//! Output formatting renderers (human-readable terminal, JSON, SARIF, JUnit, and GitHub Annotations).

pub mod github;
pub mod human;
pub mod json;
pub mod junit;
pub mod sarif;

pub use github::render as render_github;
pub use human::render as render_human;
pub use json::render as render_json;
pub use junit::render as render_junit;
pub use sarif::render as render_sarif;
