use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
};

use serde::{Deserialize, Serialize};

use bbup_rust::comunications::Basic;

fn write<T: Serialize>(socket: &mut TcpStream, content: T) -> std::io::Result<()> {
    socket.write((serde_json::to_string(&content)? + "\n").as_bytes())?;
    socket.flush()?;

    Ok(())
}

fn read<'a, T: Deserialize<'a>>(
    socket: &mut BufReader<TcpStream>,
    buffer: &'a mut String,
) -> std::io::Result<T> {
    buffer.clear();
    socket.read_line(buffer)?;
    let output: T = serde_json::from_str(buffer.as_str())?;
    Ok(output)
}

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
