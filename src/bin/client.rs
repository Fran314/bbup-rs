use std::{
    io::{Read, Write},
    net::TcpStream,
};

fn main() -> std::io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:4000")?;

    stream.write_all(b"Ehi ciao")?;
    stream.flush()?;

    let mut buffer = String::new();
    stream.read_to_string(&mut buffer)?;

    println!("Recieved: {}", buffer);

    Ok(())
}
