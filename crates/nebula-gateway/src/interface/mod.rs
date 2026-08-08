//! Nebula Gateway interface layer (UniGateway-backed).
//!
//! Protocol adapt + C5 tooling gate + C3 error envelopes.
//! Routing stays on Nebula Router.

pub mod adapt;
pub mod errors;
pub mod gate;

pub use adapt::*;
pub use errors::*;
pub use gate::*;
