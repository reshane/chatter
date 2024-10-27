use std::net::TcpStream;
use std::io::Write;
use std::io::Read;
use std::io::ErrorKind;
use std::io;
use std::thread;
use std::sync::mpsc::{
    channel,
    Sender,
    Receiver,
};

const FLAGS_LEN: usize = 1;
const AUTHOR_SIGNATURE_LEN: usize = 4;
const CONTENT_LENGTH_LEN: usize = 4;
const HEADER_LEN: usize = 9;
const TEXT_FLAG:  u8 = 0b00000001;
const IMAGE_FLAG: u8 = 0b00000010;

struct Headers {
    flags:  u8,
    author: Vec<u8>,
    length: Vec<u8>,
}

impl Headers {
    pub fn from_bytes(bytes: [u8; HEADER_LEN]) -> Self {
        let flags = bytes[0];
        let author = bytes[FLAGS_LEN..(FLAGS_LEN+AUTHOR_SIGNATURE_LEN)].to_vec();
        let length = bytes[(FLAGS_LEN+AUTHOR_SIGNATURE_LEN)..].to_vec();
        Headers {
            flags,
            author,
            length,
        }
    }
}

struct SignedData {
    author: String,
    message: Vec<u8>,
}

enum Message {
    Text(SignedData),
    Image(SignedData),
    Quit,
}

fn network_thread(mut stream: TcpStream, receiver: Receiver<Message>, sender: Sender<Message>) {
    let mut message_buf = [0; 128];
    let mut header_peek_buf = [0; HEADER_LEN];
    loop {
        // read the flags
        // peek the headers
        let peek_bytes_read = match stream.peek(&mut header_peek_buf) {
            Ok(n) => n,
            Err(_) => 0,
        };

        if peek_bytes_read == HEADER_LEN {// if we received a full header, we can process
            // we can actually consume the header into header_peek_buf to clear header data from
            // queue
            let _ = stream.read_exact(&mut header_peek_buf).map_err(|err| {
                println!("Could not read headers off the stream {}", err);
            });
            // we should parse the headers here instead of while doing the message parsing
            let headers = Headers::from_bytes(header_peek_buf);
            let author = match String::from_utf8(headers.author) {
                Ok(author) => author,
                Err(err) => { 
                    println!("Error reading author from headers {}", err);
                    String::default()
                },
            };
            let content_len: [u8; 8] = match headers.length[0..size_of::<usize>()].try_into() {
                Ok(val) => val,
                Err(err) => {
                    println!("Error parsing content length from header {}", err);
                    [0; 8]
                },
            };
            let content_len: usize = usize::from_le_bytes(content_len);
            match headers.flags {
                TEXT_FLAG => {
                    // collect all the input into a String
                    let mut full_message: Vec<u8> = vec![];
                    loop {
                        match stream.read(&mut message_buf) {
                            Ok(n) =>  {
                                if n == 0 {
                                    break;
                                }
                                full_message.append(&mut message_buf[0..n].to_vec());
                            },
                            Err(err) => {
                                match err.kind() {
                                    ErrorKind::Interrupted => {},
                                    ErrorKind::WouldBlock => {
                                        break;
                                    },
                                    _ => {
                                        println!("encountered error while reading from stream of kind {} {}", err.kind(), err);
                                        break;
                                    },
                                }
                            }
                        };
                    }
                    match String::from_utf8(full_message[1..].to_vec()) {
                        Ok(msg) => {
                            let author = match String::from_utf8(header_peek_buf[FLAGS_LEN..AUTHOR_SIGNATURE_LEN].to_vec()) {
                                Ok(a) => a,
                                Err(err) => {
                                    println!("Failed to read author signature from headers {}", err);
                                    String::default()
                                }
                            };

                            let _ = sender.send(Message::Text(
                                SignedData {
                                    author,
                                    message: msg.as_bytes().to_vec(),
                                }
                            ));
                        },
                        Err(err) => {
                            println!("Error, message is not proper utf8 {}", err);
                        },
                    };
                },
                IMAGE_FLAG => {
                    println!("We are reveicing an image! {}", message_buf[0]);
                    // collect all the input into a String
                    let mut full_message: Vec<u8> = vec![];
                    loop {
                        match stream.read(&mut message_buf) {
                            Ok(n) =>  {
                                if n == 0 {
                                    break;
                                }
                                full_message.append(&mut message_buf[0..n].to_vec());
                            },
                            Err(err) => {
                                match err.kind() {
                                    ErrorKind::Interrupted => {},
                                    ErrorKind::WouldBlock => {
                                        break;
                                    },
                                    _ => {
                                        println!("encountered error while reading from stream of kind {} {}", err.kind(), err);
                                        break;
                                    },
                                }
                            }
                        };
                    }
                    // now we should have an image file in full_message
                    // we want to use the image crate to take a look at it and maybe save it
                    // we want to grab the author id from somewhere before slurping the whole image
                    let author: String = match String::from_utf8(full_message[0..AUTHOR_SIGNATURE_LEN].to_vec()) {
                        Ok(author_name) => author_name,
                        Err(err) => {
                            println!("ERROR: author name could not be read while receiving image: {}", err);
                            String::default()
                        }
                    };

                    let _ = sender.send(Message::Image(
                        SignedData{
                            author,
                            message: full_message[AUTHOR_SIGNATURE_LEN..].to_vec(),
                        }
                    ));

                },
                _ => {
                    println!("We are receiving garbage... {}", message_buf[0]);
                },
            };
        } else {
            println!("could not receive full header, ignoring message");
        }
        match receiver.try_recv() {
            Ok(msg) => {
                match msg {
                    Message::Text(data) => {
                        let message = String::from_utf8(data.message).unwrap();
                        let formatted_msg = format!("{}: {}", data.author, message.trim_end());
                        println!("{}", formatted_msg);
                        if data.author == "self".to_string() {
                            // we also need to send it
                            let author_signature = format!("{}", stream.local_addr().expect("could not get local port").port() % 10000); // construct 4 byte author signature from port
                            let formatted_msg = format!("{} {}", author_signature, message);
                            let res = stream.write_all(&[&[TEXT_FLAG], formatted_msg.as_bytes()].concat());
                        }
                    },
                    Message::Image(data) => {
                        match data.author.as_str() {
                            "self" => {
                                let author_signature = format!("{} ", stream.local_addr().expect("could not get local port").port() % 10000); // construct 4 byte author signature from port
                                let message_headers = &[&[IMAGE_FLAG], author_signature.as_bytes()].concat();
                                let message_body = &*data.message;
                                // write_all splits things up into multiple messages :(
                                // break the message up into chunks and send one at a time
                                let res = stream.write_all(&[message_headers, message_body].concat());
                                // let _ = stream.write_all();
                            },
                            _ => {
                                println!("{} sent an image", data.author);
                                let img = image::load_from_memory(&data.message);
                                match img {
                                    Ok(img) => {
                                        println!("We have received VALID image bytes! {:?}", img);
                                    },
                                    Err(err) => {
                                        println!("We have received INVALID image bytes! {}", err);
                                    },
                                }
                            }
                        }
                    },
                    Message::Quit => {
                        break;
                    },
                }
            },
            Err(_err) => {},
        };
    }
}

fn user_thread(messages: Sender<Message>) {
    loop {
        let mut user_input_buf = String::new();
        io::stdin().read_line(&mut user_input_buf).expect("can read to user input buffer");
        if &user_input_buf == "quit" {
            let _ = messages.send(Message::Quit);
            break;
        }
        if user_input_buf.starts_with("\\image: ") {
            println!("Sending image: {}", user_input_buf);
            // match image::open(user_input_buf[8..].to_string()) {
            match image::open("/home/shnk/Projects/chatter/pep_but.png".to_string()) {
                Ok(img) => {
                    let _ = messages.send(Message::Image(
                        SignedData {
                            author: "self".to_string(),
                            message: img.as_bytes().to_vec(),
                        }
                    ));
                },
                Err(err) => {
                    println!("ERROR: could not read file from path {} {}", user_input_buf[8..].to_string(), err);
                },
            };
        } else {
            let _ = messages.send(Message::Text(
                SignedData {
                    message: user_input_buf.as_bytes().to_vec(), 
                    author: "self".to_string(),
                }
            ));
        }
    }
}

pub fn run(host: String, port: String) -> std::io::Result<()> {
    let stream = TcpStream::connect(format!("{}:{}", host, port))?;
    stream.set_nonblocking(true).expect("could not set nonblocking");

    let (message_sender, message_receiver) = channel();
    let message_sender_clone = message_sender.clone();
    let user_handle = thread::spawn(|| user_thread(message_sender_clone));
    let message_sender_clone = message_sender.clone();
    let network_handle = thread::spawn(|| network_thread(stream, message_receiver, message_sender_clone));

    user_handle.join().expect("joining user handle failed!");
    network_handle.join().expect("joining network handle failed!");

    Ok(())
}
