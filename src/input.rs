use std::io::{self, BufRead, Write};

pub fn get<T: std::fmt::Display>(prompt: T) -> std::io::Result<String> {
    let mut input = String::new();

    print!("{}", prompt);
    io::stdout().flush().unwrap();
    io::stdin().lock().read_line(&mut input)?;

    trim_newline(&mut input);

    Ok(input)
}

fn trim_newline(s: &mut String) {
    if s.ends_with('\n') {
        s.pop();
        if s.ends_with('\r') {
            s.pop();
        }
    }
}
