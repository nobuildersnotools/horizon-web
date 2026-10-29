//! The process's folders and arguments. The browser has neither a working
//! directory nor an executable: the virtual filesystem's root stands in for
//! both, and the page hands the arguments to `horizon_start`.

#[cfg(not(target_arch = "wasm32"))]
pub use std::env::{args, current_dir, current_exe, temp_dir};

/// The arguments the page started the game with, after a program name as
/// std's `args` has.
#[cfg(target_arch = "wasm32")]
pub fn args() -> std::vec::IntoIter<String> {
    let mut args = vec!["horizon-client".to_string()];
    args.extend(crate::web::args());
    args.into_iter()
}

/// The game's folder: the virtual filesystem's root.
#[cfg(target_arch = "wasm32")]
pub fn current_dir() -> std::io::Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from("/"))
}

/// A program file in the root, so its parent is the game's folder.
#[cfg(target_arch = "wasm32")]
pub fn current_exe() -> std::io::Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from("/horizon-client"))
}

/// Scratch files: a folder of the virtual filesystem.
#[cfg(target_arch = "wasm32")]
pub fn temp_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp")
}
