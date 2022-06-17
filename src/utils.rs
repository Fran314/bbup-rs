pub fn to_io_err<T: std::fmt::Debug>(e: T) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))
}

pub fn join_paths(path: Vec<&str>) -> Option<String> {
    match path.len() {
        0 => None,
        1 => Some(String::from(path[0])),
        _ => Some(String::from(
            std::path::Path::new(path[0])
                .join(join_paths(path[1..path.len()].to_vec())?)
                .to_str()?,
        )),
    }
}

#[macro_export]
macro_rules! path {
    ( $( $x:expr ),* ) => {
        {
            let mut temp_vec = Vec::new();
            $(
                temp_vec.push($x);
            )*
            bbup_rust::utils::join_paths(temp_vec).expect("Could not create a path")
        }
    };
}
