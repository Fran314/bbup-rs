use std::io::{self, BufRead, Write};

use crate::utils;

pub fn get<T: std::fmt::Display>(prompt: T) -> std::io::Result<String> {
    let mut input = String::new();

    print!("{}", prompt);
    io::stdout().flush().unwrap();
    io::stdin().lock().read_line(&mut input)?;

    utils::trim_newline(&mut input);

    Ok(input)
}
