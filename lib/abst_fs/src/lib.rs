mod error;
use error::{error_context, generr, inerr, unkext, wrgobj, Error};

mod path;
pub use path::{AbstPath, Endpoint, ObjectType};

mod directory;
pub use directory::{
    create_dir, ensure_parent, list_dir_content, make_clean_dir, remove_dir, remove_dir_all,
};

mod file;
pub use file::{
    async_create_file, async_read_file, create_file, read_file, remove_file, rename_file,
};

mod symlink;
use symlink::ABST_OBJ_HEADER;
pub use symlink::{create_symlink, read_link, remove_symlink, rename_symlink};

mod mtime;
pub use mtime::{get_mtime, set_mtime, Mtime};

mod saveload;
pub use saveload::{load, save};

mod env;
pub use env::{cwd, home_dir};
