//! Pure in-memory rules engine evaluating environment contract compliance.

pub mod contract;
pub mod engine;

pub use contract::Contract;
pub use engine::{evaluate_contract, AuditOptions};

