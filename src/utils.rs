pub fn std_err(e: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}

pub fn to_io_err<T: std::fmt::Debug>(e: T) -> std::io::Error {
    std_err(format!("{:?}", e).as_str())
}

pub fn to_exclude(
    path: &std::path::PathBuf,
    exclude_list: &Vec<regex::Regex>,
) -> std::io::Result<bool> {
    let path_str = match path.to_str() {
        None => return Err(std_err("cannot convert path to str for exclusion check")),
        Some(val) => val,
    };
    Ok(exclude_list.iter().any(|rule| rule.is_match(path_str)))
}
