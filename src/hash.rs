use sha2::{
    digest::{
        consts::{B0, B1},
        generic_array::GenericArray,
        typenum::{UInt, UTerm},
    },
    Digest, Sha256,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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
            .into_iter()
            .for_each(|byte| output += format!("{:02x}", byte).as_str());

        output[0..len as usize].to_string()
    }
}
impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex(0))
    }
}
/// Convert the absurd output type of sha2's digest/finalize
/// to a useful \[u8; 32\]
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
