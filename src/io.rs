use std::io::{self, BufRead, Write};

use crate::utils;

#[cfg(windows)]
const LINE_ENDING: &'static str = "\r\n";
#[cfg(not(windows))]
const LINE_ENDING: &'static str = "\n";

// TODO better error handling would be nice
pub fn get_input<T: std::fmt::Display>(prompt: T) -> std::io::Result<String> {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut buffer = String::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle.read_line(&mut buffer)?;

    let output = buffer
        .strip_suffix(LINE_ENDING)
        .ok_or_else(|| utils::std_err("invalid line ending"))?
        .to_string();

    Ok(output)
}
