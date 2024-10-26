use std::net::{SocketAddr, TcpStream, TcpListener};
use std::io::{Write, Read};
use std::fmt;
use std::result;
use std::thread;
use std::ops::Deref;
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::mpsc::{
    channel,
    Sender,
    Receiver,
};

type Result<T> = result::Result<T, ()>;

const SAFE_MODE: bool = true;

struct Sensitive<T>(T);

enum Message {
    ClientConnected(Arc<TcpStream>),
    ClientDisconnected(SocketAddr),
    NewMessage(SocketAddr, Vec<u8>),
}

impl <T: fmt::Display> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(inner) = self;
        if SAFE_MODE {
            writeln!(f, "[REDACTED]")
        } else {
            writeln!(f, "{inner}")
        }
    }
}

struct Client {
    conn: Arc<TcpStream>,
}

fn server(messages: Receiver<Message>) -> Result<()> {
    let mut clients: HashMap<SocketAddr, Client> = HashMap::new();
    loop {
        let msg = messages.recv().expect("The server receiver is not hung up");
        match msg {
            Message::ClientConnected(stream) => {
                let client = Client {
                    conn: stream.clone(),
                };
                let author_addr = stream.as_ref().peer_addr().unwrap();
                clients.insert(author_addr, client);
                println!("INFO: new client connected: {:?}", author_addr);
            },
            Message::ClientDisconnected(author) => {
                println!("INFO: client disconnected: {:?}", author);
                let _ = clients.remove(&author);
            },
            Message::NewMessage(author, bytes) => {
                println!("INFO: new message from {author:?}: {:?}", bytes);
                for (addr, stream) in clients.iter() {
                    if *addr != author {
                        clients.get(addr)
                            .expect("key is still in the map")
                            .conn.deref().write(&bytes).unwrap();
                    }
                }
            },
        }
    }
}

fn client(mut stream: Arc<TcpStream>, messages: Sender<Message>) -> Result<()> {
    let author = stream.peer_addr().map_err(|err| {
        eprintln!("ERROR: could not get client address");
    })?;
    messages.send(Message::ClientConnected(stream.clone())).map_err(|err| {
        eprintln!("ERROR: could not send message to server thread: {err}");
    })?;
    let mut buffer = Vec::new();
    buffer.resize(64, 0);
    loop {
        let n = stream.deref().read(&mut buffer).map_err(|err| {
            eprintln!("ERROR could not read message from client: {err}");
            let _ = messages.send(Message::ClientDisconnected(author));
        })?;
        if n != 0 {
            messages.send(
                Message::NewMessage(
                    stream.as_ref().peer_addr().unwrap(), 
                    buffer[0..n].to_vec()
                    )
                ).map_err(|err| {
                eprintln!("ERROR: could not send message to server thread: {err}");
            })?;
        } else {
            let _ = messages.send( Message::ClientDisconnected(author));
            return Ok(());
        }
    }
}

pub fn run(host: String, port: String) -> Result<()> { 
    let address = "127.0.0.1:6969";
    let listener = TcpListener::bind(address).map_err(|err| {
        eprintln!("ERROR: server unable to bind {address}: {err}", err = Sensitive(err));
    }).unwrap();
    println!("INFO: listening to {}", Sensitive(address));

    let (message_sender, message_receiver) = channel();
    thread::spawn(|| server(message_receiver));

    for stream in listener.incoming() {
        let message_sender = message_sender.clone();
        match stream {
            Ok(mut stream) => {
                let stream = Arc::new(stream);
                thread::spawn(move || client(stream.clone(), message_sender));
            },
            Err(err) => {
                eprintln!("ERROR: could not accept connection: {err}");
            }
        }
    }

    Ok(())
}


