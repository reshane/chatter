use std::env;
use std::io::ErrorKind;

mod client;
mod server;

fn print_usage(program: String) {
    println!("Usage: {program} mode host port");
    println!("\tmode: server | client");
    println!("\thost: defaults to 127.0.0.1");
    println!("\tport: defaults to 6969");
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        print_usage(args[0].clone());
        // hack to get it to return an error
        return Err(std::io::Error::new(ErrorKind::NotFound,
            "Missing HOST and or PORT arguments"));
    }

    let host = args[2].to_string();
    let port = args[3].to_string();

    match args[1].as_str() {
        "client" => {
            client::run(host, port);
        },
        "server" => {
            server::run(host, port);
        },
        _ => {
            print_usage(args[0].clone());
        return Err(std::io::Error::new(ErrorKind::NotFound,
            "Either must provide either client or server mode"));

        },
    }

    Ok(())
}
