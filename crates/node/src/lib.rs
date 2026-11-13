//! The node side of saarthi: everything `saarthi node serve` runs, as a library so the hub's
//! end-to-end tests can drive a real node.

pub mod claude_api;
pub mod containment;
pub mod health;
pub mod hooks;
pub mod link;
pub mod outbox;
pub mod pending;
pub mod prompt;
pub mod registry;
pub mod run;
pub mod sessions;
pub mod sink;
pub mod spawner;
pub mod transcript;
pub mod wrapper;
