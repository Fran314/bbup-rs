mod error;
mod mtime;
mod path;
mod saveload;
mod utility;
use error::{error_context, generr, inerr, unkext, wrgobj, Error};
pub use mtime::{get_mtime, set_mtime, Mtime};
use path::ABST_OBJ_HEADER;
pub use path::{AbstPath, Endpoint, ObjectType};
pub use saveload::{load, save};
pub use utility::*;

//--- LOACAL INFORMATIONS ---//
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
//--- ---//
