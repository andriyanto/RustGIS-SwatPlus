//! L3: runs a SWAT+ project with the [swatplus-rust](https://github.com/andriyanto/swatplus-rust)
//! engine (A1.1 of its roadmap).
//!
//! The engine is LGPL-2.1, as SWAT+ is; RustGIS-SwatPlus is Apache-2.0. So the engine is never
//! linked: natively it is the `swatplus-rust` program, run as a subprocess ([`runner`] finds it,
//! [`install`] downloads the pinned release), and the two meet through files and the program's
//! JSON-line events ([`run`]). In the browser the engine's WebAssembly module will run it (not yet).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod event;
#[cfg(not(target_arch = "wasm32"))]
pub mod install;
#[cfg(not(target_arch = "wasm32"))]
pub mod run;
#[cfg(not(target_arch = "wasm32"))]
pub mod runner;

pub use event::Event;
