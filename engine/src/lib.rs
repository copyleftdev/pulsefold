//! Pulsefold engine: the reference, the implementation variants, the oracle, sessions, the
//! seeded simulator, the planted-bug catalog and deterministic races. The `pulsefold` binary
//! and the WebAssembly build in the browser console both drive it through these modules.
pub mod api;
pub mod assay;
pub mod catalog;
pub mod marker;
pub mod model;
pub mod oracle;
pub mod race;
pub mod rng;
pub mod session;
pub mod sim;
pub mod stores;
#[cfg(target_arch = "wasm32")]
mod wasm;
