//! Output formatting renderers (human-readable terminal and JSON).

pub mod human;
pub mod json;

pub use human::render as render_human;
pub use json::render as render_json;

