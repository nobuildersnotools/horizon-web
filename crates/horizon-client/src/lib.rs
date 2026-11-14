//! Gameplay and startup, above the reusable client library.
// No `#![recursion_limit]` or other inner attribute horizon-mixins doesn't know here: the
// launcher's horizon-mixins then rejects every native mod targeting this crate (#8169).
#![forbid(unsafe_code)]
extern crate self as horizon_client;
// The browser's stand-in for ureq: the same calls, every request failing as if offline.
#[cfg(target_arch = "wasm32")]
extern crate horizon_ureq_web as ureq;
pub use horizon_client_lib::*;
pub mod entry;
pub mod game;
mod horizon_look;
pub mod mod_api;

#[cfg(test)]
mod brewing_speed_load_tests;
#[cfg(test)]
mod living_death_tests;
