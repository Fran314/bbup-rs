use std::{io::BufReader, net::TcpStream};

use bbup_rust::comunications::syncrw::{read, write};
use bbup_rust::comunications::Basic;

fn main() -> std::io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:3000")?;

    let mut input = String::new();
    let mut reader = BufReader::new(stream.try_clone()?);

    let read_value: Basic = read(&mut reader, &mut input)?;
    println!("Recieved from server: {}", read_value.content);

    write(&mut stream, Basic::new("AAAA"))?;

    let read_value: Basic = read(&mut reader, &mut input)?;
    println!("Recieved from server: {}", read_value.content);

    let read_value: Basic = read(&mut reader, &mut input)?;
    println!("Recieved from server: {}", read_value.content);

    Ok(())
}
