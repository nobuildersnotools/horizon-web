//! The process's folders. The browser has neither a working directory nor an
//! executable: the virtual filesystem's root stands in for both.

#[cfg(not(target_arch = "wasm32"))]
pub use std::env::{current_dir, current_exe, temp_dir};

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
