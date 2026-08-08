//! Nebula Gateway interface layer (UniGateway-backed).
//!
//! Protocol adapt + C5 tooling gate. Routing stays on Nebula Router.

pub mod adapt;
pub mod gate;

pub use adapt::*;
pub use gate::*;
