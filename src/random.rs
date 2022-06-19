use rand::Rng;

pub fn random_hex(len: usize) -> String {
    const CHARSET: &[u8] = b"0123456789abcdef";
    let mut rng = rand::thread_rng();

    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
