use std::io::{self, BufRead, Write};

// TODO better error handling would be nice
pub fn get_input<T: std::fmt::Display>(prompt: T) -> std::io::Result<String> {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut buffer = String::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle.read_line(&mut buffer)?;

    buffer.pop();
    Ok(buffer)
}
