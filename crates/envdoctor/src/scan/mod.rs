//! Source code AST scanners.

pub mod js;
pub mod rust;

pub use js::ScanResult;
use crate::model::path::RelPath;

/// Unified entry point to scan source code for environment variable accesses.
///
/// Dispatches to the appropriate AST scanner based on file extension.
pub fn scan_source(source_text: &str, path: impl Into<RelPath>) -> ScanResult {
    let rel_path = path.into();
    if rel_path.as_path().extension().and_then(|ext| ext.to_str()) == Some("rs") {
        rust::scan_source(source_text, rel_path)
    } else {
        js::scan_source(source_text, rel_path)
    }
}
