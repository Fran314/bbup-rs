use super::{generr, inerr, AbstPath, Error};

pub fn home_dir() -> Result<AbstPath, Error> {
    match dirs::home_dir() {
        Some(home_dir) => Ok(AbstPath::from(home_dir)),
        None => Err(generr(
            "unable to retrieve home directory path",
            "failed to get home directory through crate `dirs`",
        )),
    }
}
pub fn cwd() -> Result<AbstPath, Error> {
    Ok(AbstPath::from(std::env::current_dir().map_err(inerr(
        "failed to retrieve current working directory",
    ))?))
}

#[cfg(test)]
mod tests {
    use super::{cwd, home_dir};

    #[test]
    fn test() {
        // I have yet to find a better way to test these two functions, so
        //	I'm stealing this method from the crate from which I'm also stealing
        //	the home_dir function, which is dirs
        // dirs: https://docs.rs/dirs/latest/dirs/
        println!("home directory:            {}", home_dir().unwrap());
        println!("current working directory: {}", cwd().unwrap());
    }
}
