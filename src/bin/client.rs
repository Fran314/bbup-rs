use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
};

fn main() -> std::io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:3000")?;

    let mut input = String::new();
    let mut reader = BufReader::new(stream.try_clone()?);
    reader.read_line(&mut input)?;
    println!("Recieved from server: {}", input);

    stream.write(b"{ \"id\": \"31415\", \"content\": \"AAAAAAAAA\"}\n")?;
    stream.flush()?;

    input.clear();
    reader.read_line(&mut input)?;
    println!("Recieved from server: {}", input);

    Ok(())
}
