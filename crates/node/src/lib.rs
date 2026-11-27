//! The node side of saarthi: everything `saarthi node serve` runs, as a library. An app
//! builds and runs a node of its own with [`node::Node`] (ADR-0080); the hub's end-to-end
//! tests drive a real node from its parts.

pub mod awake;
pub mod changes;
pub mod claude_api;
pub mod containment;
pub mod health;
pub mod hooks;
pub mod link;
pub mod node;
pub mod outbox;
pub mod pending;
pub mod prompt;
pub mod questions;
pub mod queue;
pub mod registry;
pub mod run;
pub mod services;
pub mod shells;
pub mod sessions;
pub mod sink;
pub mod spawner;
pub mod terminal;
pub mod transcript;
pub mod usage;
pub mod uploads;
pub mod wrapper;
