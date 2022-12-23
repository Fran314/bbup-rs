use sha2::{
    digest::{
        consts::{B0, B1},
        generic_array::GenericArray,
        typenum::{UInt, UTerm},
    },
    Digest, Sha256,
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn to_bytes(&self) -> Vec<u8> {
        let Hash(bytes) = self;
        bytes.to_vec()
    }
    pub fn to_hex(&self, len: u8) -> String {
        let len = {
            if len > 0 && len <= 64 {
                len
            } else {
                64
            }
        };
        let Hash(bytes) = self;
        let mut output = String::new();
        bytes
            .iter()
            .for_each(|byte| output += format!("{:02x}", byte).as_str());

        output[0..len as usize].to_string()
    }
}
impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::fmt::Debug for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex(0))
    }
}
/// Convert the absurd output type of sha2's digest/finalize
/// to a useful Hash
#[allow(clippy::type_complexity)]
fn to_hash(
    hash: GenericArray<u8, UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B1>, B0>, B0>, B0>, B0>, B0>>,
) -> Hash {
    Hash(hash.as_slice().try_into().unwrap())
}

/// Hash anything that can be converted to u8 array (usually
/// Strings or &str)
pub fn hash_bytes<T: std::convert::AsRef<[u8]>>(s: T) -> Hash {
    to_hash(Sha256::digest(s))
}

/// Hash anything that can be streamed (usually files)
pub fn hash_stream<T: std::io::Read>(mut stream: T) -> std::io::Result<Hash> {
    let mut hasher = Sha256::new();
    match std::io::copy(&mut stream, &mut hasher) {
        Ok(_) => Ok(to_hash(hasher.finalize())),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{hash_bytes, hash_stream, Hash};

    #[test]
    fn test() {
        to_bytes();
        to_string();
        hash();
    }

    fn to_bytes() {
        for _ in 0..1000 {
            let bytes: [u8; 32] = rand::random();
            let other: [u8; 32] = rand::random();
            assert_eq!(Hash(bytes).to_bytes(), bytes.to_vec());
            if bytes != other {
                assert_ne!(Hash(bytes).to_bytes(), other);
            }
        }
    }

    fn to_string() {
        for b in 0..=15 {
            let h = Hash([17 * b; 32]);
            let s = format!("{b:x}");
            for i in 1..64 {
                assert_eq!(h.to_hex(i), s.repeat(i as usize));
            }
            assert_eq!(h.to_hex(0), s.repeat(64));
            assert_eq!(h.to_hex(65), s.repeat(64));
        }
        for b in 0..=255 {
            let h = Hash([b; 32]);
            let s = format!("{b:02x}");
            for i in 1..32 {
                assert_eq!(h.to_hex(i * 2), s.repeat(i as usize));
            }
            assert_eq!(h.to_hex(0), s.repeat(32));
            assert_eq!(h.to_hex(65), s.repeat(32));
        }
        for _ in 0..1000 {
            let bytes: [u8; 32] = rand::random();
            let mut s = String::new();
            for byte in bytes {
                s += format!("{byte:02x}").as_str();
            }
            assert_eq!(Hash(bytes).to_string(), s);
            assert_eq!(format!("{}", Hash(bytes)), s);
            assert_eq!(format!("{:?}", Hash(bytes)), s);
        }
    }

    fn hash() {
        let tests = [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "here is some random text",
                "3ace1cf028afc2c9872ec0eb6fd25b6a083264de078e9d8459b7ea90954d52fa",
            ),
            (
                "and also a different text",
                "549f713ae4bbf70c48c4aa6a0c9b55af40ba51dd86ebcd7c77d345cdd5fe5cca",
            ),
            (
                "boop beep boop bzzzz am robot executing tests",
                "117a49851674557df82e276d46fe24453808d8bd7ada0f11142dee8ddec3ae06",
            ),
        ];
        for (text, hash_val) in tests {
            assert_eq!(hash_bytes(text).to_string(), hash_val);
            assert_eq!(
                hash_stream(std::io::Cursor::new(text)).unwrap().to_string(),
                hash_val
            );
        }
    }
}
