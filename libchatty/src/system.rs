use std::{
    ffi::OsString,
    path::PathBuf
};

pub fn get_user_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap()
        .join("aluminum")
}

pub fn get_default_path() -> OsString {
    get_user_dir()
        .join("user.db")
        .into_os_string()
}

pub fn get_relay_path() -> PathBuf {
    get_user_dir()
        .join("relay.toml")
}
