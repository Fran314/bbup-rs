// use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};

use super::ensure_parent;
use super::{error_context, generr, inerr, unkext, AbstPath, Error, ObjectType};

enum Ext {
    // JSON,
    // YAML,
    Bin,
    Toml,
}
fn get_ext(path: &AbstPath) -> Option<Ext> {
    // let ext = match path.as_ref().extension().and_then(std::ffi::OsStr::to_str) {
    //     Some(val) => val,
    //     None => return None,
    // };
    let ext = path.extension()?;
    match ext.to_ascii_lowercase().as_str() {
        // "json" => Some(Ext::JSON),
        // "yaml" => Some(Ext::YAML),
        "bin" => Some(Ext::Bin),
        "toml" => Some(Ext::Toml),
        _ => None,
    }
}

/// Load the data from a file, interpreting the content of the file based on the
/// extension (see [`Ext`] for the possible extensions) of the file and deserializing
/// the content to the generic type T
pub fn load<T: DeserializeOwned>(path: &AbstPath) -> Result<T, Error> {
    let errctx = error_context(format!("could not load file at path {}", path));
    if !path.exists() {
        return Err(generr(errctx("open file"), "file doesn't exist"));
    }
    if path.object_type() != Some(ObjectType::File) {
        return Err(generr(errctx("open file"), "object at path is not a file"));
    }

    match get_ext(path) {
        // Some(Ext::JSON) => {
        //     let serialized =
        //         std::fs::read_to_string(&path).map_err(inerr(errctx("read content to string")))?;
        //     serde_json::from_str(&serialized)
        //         .map_err(inerr(errctx("deserialize content from json")))
        // }
        // Some(Ext::YAML) => {
        //     let serialized =
        //         std::fs::read_to_string(&path).map_err(inerr(errctx("read content to string")))?;
        //     serde_yaml::from_str(&serialized)
        //         .map_err(inerr(errctx("deserialize content from yaml")))
        // }
        Some(Ext::Toml) => {
            let serialized = std::fs::read_to_string(path.to_path_buf())
                .map_err(inerr(errctx("read content to string")))?;
            toml::from_str(&serialized).map_err(inerr(errctx("deserialize content from toml")))
        }
        Some(Ext::Bin) => {
            let file =
                std::fs::File::open(path.to_path_buf()).map_err(inerr(errctx("open file")))?;
            bincode::deserialize_from(file)
                .map_err(inerr(errctx("deserialize content from binary")))
        }
        None => Err(unkext(path)),
    }
}
/// Save a serializable data structure of generic type T to a file, encoding the
/// serialized data based on the extension of the file (see [`Ext`] for the possible
/// extensions)
pub fn save<T: Serialize>(path: &AbstPath, content: &T) -> Result<(), Error> {
    let errctx = error_context(format!("could not save file at path {}", path));
    match get_ext(path) {
        // Some(Ext::JSON) => {
        //     let serialized = serde_json::to_string(content)
        //         .map_err(inerr(errctx("serialize content to json")))?;
        //     ensure_parent(&path)?;
        //     std::fs::write(&path, serialized).map_err(inerr(errctx("write content to file")))
        // }
        // Some(Ext::YAML) => {
        //     let serialized = serde_yaml::to_string(content)
        //         .map_err(inerr(errctx("serialize content to yaml")))?;
        //     ensure_parent(&path)?;
        //     std::fs::write(&path, serialized).map_err(inerr(errctx("write content to file")))
        // }
        Some(Ext::Toml) => {
            let serialized =
                toml::to_string(content).map_err(inerr(errctx("serialize content to toml")))?;
            ensure_parent(path)?;
            std::fs::write(path.to_path_buf(), serialized)
                .map_err(inerr(errctx("write content to file")))
        }
        Some(Ext::Bin) => {
            let serialized = bincode::serialize(content)
                .map_err(inerr(errctx("serialize content to binary")))?;
            ensure_parent(path)?;
            std::fs::write(path.to_path_buf(), serialized)
                .map_err(inerr(errctx("write content to file")))
        }
        None => Err(unkext(path)),
    }
}
