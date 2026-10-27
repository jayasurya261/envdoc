//! Source code AST scanners and unified Scanner trait architecture.

#[cfg(feature = "lang-go")]
pub mod go;
#[cfg(feature = "lang-infra")]
pub mod infra;
pub mod js;
pub mod prefilter;
#[cfg(feature = "lang-python")]
pub mod python;
pub mod rust;

use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::model::path::RelPath;
use crate::model::span::{Location, Span};
use std::panic::AssertUnwindSafe;
use std::sync::mpsc;
use std::time::Duration;

pub use js::ScanResult;

/// Unified Scanner trait allowing addition of new language scanners cleanly.
pub trait Scanner: Send + Sync {
    /// Human-readable identifier of this scanner.
    fn name(&self) -> &'static str;

    /// Returns `true` if this scanner can process files with the given extension or file name.
    fn can_scan(&self, ext: &str, file_name: &str) -> bool;

    /// Scans the provided source text and returns extracted references and diagnostics.
    fn scan(&self, source_text: &str, path: &str) -> ScanResult;
}

pub struct RustScanner;
impl Scanner for RustScanner {
    fn name(&self) -> &'static str {
        "rust"
    }
    fn can_scan(&self, ext: &str, _file_name: &str) -> bool {
        ext == "rs"
    }
    fn scan(&self, source_text: &str, path: &str) -> ScanResult {
        rust::scan_source(source_text, path)
    }
}

pub struct JsTsScanner;
impl Scanner for JsTsScanner {
    fn name(&self) -> &'static str {
        "javascript/typescript"
    }
    fn can_scan(&self, ext: &str, _file_name: &str) -> bool {
        matches!(
            ext,
            "js" | "mjs"
                | "cjs"
                | "jsx"
                | "ts"
                | "mts"
                | "cts"
                | "tsx"
                | "vue"
                | "svelte"
                | "astro"
        )
    }
    fn scan(&self, source_text: &str, path: &str) -> ScanResult {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let text = if matches!(ext, "vue" | "svelte" | "astro") {
            extract_script_content(source_text)
        } else {
            source_text.to_string()
        };
        js::scan_source(&text, path)
    }
}

#[cfg(feature = "lang-python")]
pub struct PythonScanner;
#[cfg(feature = "lang-python")]
impl Scanner for PythonScanner {
    fn name(&self) -> &'static str {
        "python"
    }
    fn can_scan(&self, ext: &str, _file_name: &str) -> bool {
        matches!(ext, "py" | "pyi" | "pyw")
    }
    fn scan(&self, source_text: &str, path: &str) -> ScanResult {
        python::scan_source(source_text, path)
    }
}

#[cfg(feature = "lang-go")]
pub struct GoScanner;
#[cfg(feature = "lang-go")]
impl Scanner for GoScanner {
    fn name(&self) -> &'static str {
        "go"
    }
    fn can_scan(&self, ext: &str, _file_name: &str) -> bool {
        ext == "go"
    }
    fn scan(&self, source_text: &str, path: &str) -> ScanResult {
        go::scan_source(source_text, path)
    }
}

#[cfg(feature = "lang-infra")]
pub struct InfraScanner;
#[cfg(feature = "lang-infra")]
impl Scanner for InfraScanner {
    fn name(&self) -> &'static str {
        "infra"
    }
    fn can_scan(&self, ext: &str, file_name: &str) -> bool {
        let lower = file_name.to_ascii_lowercase();
        lower == "dockerfile"
            || lower.starts_with("dockerfile.")
            || lower.ends_with(".dockerfile")
            || lower == "docker-compose.yml"
            || lower == "docker-compose.yaml"
            || lower == "compose.yml"
            || lower == "compose.yaml"
            || matches!(ext, "sh" | "bash" | "zsh" | "tf" | "tfvars")
            || matches!(ext, "yml" | "yaml")
    }
    fn scan(&self, source_text: &str, path: &str) -> ScanResult {
        infra::scan_infra_source(source_text, path)
    }
}

/// Returns all default scanners registered in EnvDoctor.
pub fn registered_scanners() -> Vec<Box<dyn Scanner>> {
    let mut scanners: Vec<Box<dyn Scanner>> = vec![Box::new(RustScanner), Box::new(JsTsScanner)];
    #[cfg(feature = "lang-python")]
    scanners.push(Box::new(PythonScanner));
    #[cfg(feature = "lang-go")]
    scanners.push(Box::new(GoScanner));
    #[cfg(feature = "lang-infra")]
    scanners.push(Box::new(InfraScanner));
    scanners
}

/// Unified entry point to scan source code for environment variable accesses.
///
/// Dispatches to the appropriate AST scanner based on file extension and name.
/// Runs in a thread with an isolated 4MB stack and a 5-second per-file timeout
/// to prevent stack overflows on deeply nested ASTs or parser lockups.
pub fn scan_source(source_text: &str, path: impl Into<RelPath>) -> ScanResult {
    let rel_path = path.into();
    let ext = rel_path
        .as_path()
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let file_name = rel_path
        .as_path()
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    // Performance pre-filter: quick byte search avoids parsing or spawning threads
    // if the file does not contain any candidate keywords for environment variables.
    if !prefilter::quick_prefilter(source_text.as_bytes(), ext, file_name) {
        return ScanResult {
            references: Vec::new(),
            diagnostics: Vec::new(),
        };
    }

    let text = if matches!(ext, "vue" | "svelte" | "astro") {
        extract_script_content(source_text)
    } else {
        source_text.to_string()
    };

    let path_clone = rel_path.to_string();
    let ext_owned = ext.to_string();
    let file_name_owned = file_name.to_string();
    let text_clone = text.clone();

    let (sender, receiver) = mpsc::channel();
    let builder = std::thread::Builder::new()
        .name("envdoc-ast-scanner".into())
        .stack_size(4 * 1024 * 1024);

    let handle = builder.spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let scanners = registered_scanners();
            for scanner in scanners {
                if scanner.can_scan(&ext_owned, &file_name_owned) {
                    return scanner.scan(&text_clone, path_clone.as_str());
                }
            }
            // Fallback: try JS scanner
            js::scan_source(&text_clone, path_clone.as_str())
        }));
        let _ = sender.send(result);
    });

    match handle {
        Ok(_) => match receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(scan_result)) => scan_result,
            Ok(Err(_panic_err)) => ScanResult {
                references: Vec::new(),
                diagnostics: vec![Diagnostic::new(
                    DiagnosticCode::ENV011,
                    Severity::Warning,
                    "Parser panicked on deeply nested or malformed syntax; recovered safely"
                        .to_string(),
                    None,
                    vec![Location::new(rel_path, Span::point(1, 1, 0))],
                    None,
                )],
            },
            Err(_) => ScanResult {
                references: Vec::new(),
                diagnostics: vec![Diagnostic::new(
                    DiagnosticCode::ENV011,
                    Severity::Warning,
                    "Parser timed out after 5s; file may be adversarial or excessively nested"
                        .to_string(),
                    None,
                    vec![Location::new(rel_path, Span::point(1, 1, 0))],
                    None,
                )],
            },
        },
        Err(_) => {
            // Fallback in-thread if thread spawn fails
            let ext_clone = ext.to_string();
            let file_name_clone = file_name.to_string();
            let fallback_res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let scanners = registered_scanners();
                for scanner in scanners {
                    if scanner.can_scan(&ext_clone, &file_name_clone) {
                        return scanner.scan(&text, rel_path.as_str());
                    }
                }
                js::scan_source(&text, rel_path.as_str())
            }));
            fallback_res.unwrap_or_else(|_| ScanResult {
                references: Vec::new(),
                diagnostics: vec![Diagnostic::new(
                    DiagnosticCode::ENV011,
                    Severity::Warning,
                    "Parser panicked on malformed input; recovered safely".to_string(),
                    None,
                    vec![Location::new(rel_path, Span::point(1, 1, 0))],
                    None,
                )],
            })
        }
    }
}

/// Extracts script tags from Vue, Svelte, or Astro files for AST scanning.
fn extract_script_content(source: &str) -> String {
    let mut extracted = String::new();
    let mut remainder = source;

    while let Some(start_idx) = remainder.find("<script") {
        let after_script = &remainder[start_idx..];
        if let Some(tag_end) = after_script.find('>') {
            let script_body_start = tag_end + 1;
            let after_tag = &after_script[script_body_start..];
            if let Some(close_tag) = after_tag.find("</script>") {
                extracted.push_str(&after_tag[..close_tag]);
                extracted.push('\n');
                remainder = &after_tag[close_tag + 9..];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if extracted.trim().is_empty() {
        source.to_string()
    } else {
        extracted
    }
}
