pub fn std_err(e: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}

pub fn to_io_err<T: std::fmt::Debug>(e: T) -> std::io::Error {
    std_err(format!("{:?}", e).as_str())
}
