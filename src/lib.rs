mod partition;
mod payload;

pub use partition::*;
pub use payload::{Header, Payload};

pub mod update_metadata {
    include!(concat!(env!("OUT_DIR"), "/chromeos_update_engine.rs"));
}
