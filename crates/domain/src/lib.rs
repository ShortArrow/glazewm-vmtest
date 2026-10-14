//! Domain model of the harness: what a scenario observes and how it is judged.
//!
//! Knows nothing about virtualization platforms or the SUT's implementation.

pub mod baseline;
pub mod invariant;
pub mod scenario;
pub mod settle;
pub mod topology;
