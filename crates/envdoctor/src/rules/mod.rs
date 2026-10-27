//! Pure in-memory rules engine evaluating environment contract compliance.

pub mod contract;
pub mod engine;
pub mod typo;

pub use contract::Contract;
pub use engine::{evaluate_contract, AuditOptions};
