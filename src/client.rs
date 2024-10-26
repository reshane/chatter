use std::net::TcpStream;
use std::env;
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

const TEXT_FLAG:  u8 = 0b00000001;
const IMAGE_FLAG: u8 = 0b00000010;

struct SignedData {
    message: Vec<u8>,
    author: String,
}

enum Message {
    Text(SignedData),
    Image(SignedData),
    Quit,
}

fn network_thread(mut stream: TcpStream, receiver: Receiver<Message>, sender: Sender<Message>) {
    let mut message_buf = [0; 5];
    loop {
        // read the flags
        let num_bytes_read = match stream.read(&mut message_buf) {
            Ok(n) => n,
            Err(_) => 0,
        };

        if num_bytes_read > 0 {
            match message_buf[0] {
                TEXT_FLAG => {
                    // collect all the input into a String
                    let mut full_message: Vec<u8> = message_buf[0..num_bytes_read].to_vec();
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
                            let message: Vec<String> = msg.split(':').map(|s| s.to_string()).collect();
                            println!("Sending message to io handler {:?}", message);
                            let _ = sender.send(Message::Text(
                                SignedData {
                                    message: message[1].as_bytes().to_vec(),
                                    author: message[0].clone(),
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
                },
                _ => {
                    println!("We are receiving garbage... {}", message_buf[0]);
                },
            };
        }
        match receiver.try_recv() {
            Ok(msg) => {
                match msg {
                    Message::Text(data) => {
                        let message = String::from_utf8(data.message).unwrap();
                        let formatted_msg = format!("{}: {}", data.author, message);
                        println!("{}", formatted_msg);
                        if data.author == "self".to_string() {
                            // we also need to send it
                            let formatted_msg = format!("{}: {}", stream.local_addr().unwrap().port(), message);
                            let res = stream.write_all(&[&[TEXT_FLAG], formatted_msg.as_bytes()].concat());
                            // println!("{:?}", res);
                        }
                    },
                    Message::Image(_data) => {
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
        let _ = messages.send(Message::Text(
            SignedData {
                message: user_input_buf.as_bytes().to_vec(), 
                author: "self".to_string(),
            }
        ));
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
