use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::model::definition::Definition;
use crate::model::reference::Reference;
use crate::model::span::Location;
use crate::rules::contract::Contract;
use std::collections::{HashMap, HashSet};

/// Standard platform variables ignored by default to reduce noise on system/runtime variables.
pub const DEFAULT_PLATFORM_VARS: &[&str] = &[
    "NODE_ENV",
    "CI",
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "TEMP",
    "TMP",
    "TMPDIR",
    "RUST_LOG",
    "RUST_BACKTRACE",
    "DEBUG",
    "PORT",
    "HOSTNAME",
];

/// Standard platform prefixes ignored by default.
pub const DEFAULT_PLATFORM_PREFIXES: &[&str] = &[
    "GITHUB_",
    "VERCEL_",
    "NETLIFY_",
    "AWS_",
    "CARGO_",
    "NEXT_PUBLIC_",
];

/// Options for configuring contract evaluation.
#[derive(Debug, Clone)]
pub struct AuditOptions {
    /// Environment variable keys to ignore during audit (e.g. `NODE_ENV`, `PATH`).
    pub ignore_keys: HashSet<String>,
    /// Environment variable key prefixes to ignore (e.g. `GITHUB_`).
    pub ignore_prefixes: Vec<String>,
    /// Whether to ignore standard platform variables.
    pub ignore_platform_vars: bool,
    /// Whether environment variable comparisons are case-insensitive.
    pub case_insensitive: bool,
}

impl Default for AuditOptions {
    fn default() -> Self {
        Self {
            ignore_keys: HashSet::new(),
            ignore_prefixes: DEFAULT_PLATFORM_PREFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ignore_platform_vars: true,
            #[cfg(windows)]
            case_insensitive: true,
            #[cfg(not(windows))]
            case_insensitive: false,
        }
    }
}

impl AuditOptions {
    /// Checks if a given environment variable key should be ignored.
    pub fn is_ignored(&self, key: &str) -> bool {
        if self.ignore_keys.iter().any(|k| {
            if self.case_insensitive {
                k.eq_ignore_ascii_case(key)
            } else {
                k == key
            }
        }) {
            return true;
        }

        if self.ignore_platform_vars
            && DEFAULT_PLATFORM_VARS.iter().any(|pv| {
                if self.case_insensitive {
                    pv.eq_ignore_ascii_case(key)
                } else {
                    *pv == key
                }
            })
        {
            return true;
        }

        for prefix in &self.ignore_prefixes {
            if self.case_insensitive {
                if key
                    .to_ascii_uppercase()
                    .starts_with(&prefix.to_ascii_uppercase())
                {
                    return true;
                }
            } else if key.starts_with(prefix) {
                return true;
            }
        }

        false
    }
}

/// Evaluates contracts, active environment definitions, and source code references,
/// producing deterministically sorted diagnostics.
///
/// Pure in-memory function: zero file I/O.
pub fn evaluate_contract(
    contract: Option<&Contract>,
    env_definitions: Option<&[Definition]>,
    references: &[Reference],
    options: &AuditOptions,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Rule 5: Contract missing (ENV020)
    let contract = match contract {
        Some(c) => c,
        None => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV020,
                Severity::Error,
                "Specified contract file was not found",
                None,
                vec![],
                None,
            ));
            return diagnostics;
        }
    };

    // Rule 5: Contract empty (ENV021)
    if contract.is_empty() {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::ENV021,
            Severity::Warning,
            format!(
                "Specified contract file '{}' contains zero variable declarations",
                contract.path
            ),
            None,
            vec![Location::new(
                contract.path.clone(),
                crate::model::span::Span::point(1, 1, 0),
            )],
            None,
        ));
    }

    let contract_map = contract.definition_map();

    // Build reference map: key -> list of references
    let mut ref_map: HashMap<&str, Vec<&Reference>> = HashMap::new();
    for r in references {
        if !options.is_ignored(r.name.as_str()) {
            ref_map.entry(r.name.as_str()).or_default().push(r);
        }
    }

    // Rule 1: Undocumented reference (ENV001)
    // Environment variable accessed in source code but absent from .env.example
    for r in references {
        let key_str = r.name.as_str();
        if options.is_ignored(key_str) {
            continue;
        }

        let is_in_contract = if options.case_insensitive {
            contract
                .definitions
                .iter()
                .any(|d| d.name.as_str().eq_ignore_ascii_case(key_str))
        } else {
            contract_map.contains_key(key_str)
        };

        if !is_in_contract {
            let mut msg = format!(
                "Environment variable '{}' referenced in source code is not documented in contract '{}'",
                key_str, contract.path
            );
            if let Some(suggestion) = crate::rules::typo::find_typo_suggestion(
                key_str,
                contract.definitions.iter().map(|d| d.name.as_str()),
            ) {
                msg.push_str(&format!(" (did you mean '{}'?)", suggestion));
            }

            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV001,
                Severity::Warning,
                msg,
                Some(r.name.clone()),
                vec![r.location.clone()],
                Some(r.evidence_kind),
            ));
        }
    }

    // Rule 2: Unused definition (ENV002)
    // Defined in .env.example but not referenced anywhere in scanned source
    for def in &contract.definitions {
        let key_str = def.name.as_str();
        if options.is_ignored(key_str) {
            continue;
        }

        let is_referenced = if options.case_insensitive {
            references
                .iter()
                .any(|r| r.name.as_str().eq_ignore_ascii_case(key_str))
        } else {
            ref_map.contains_key(key_str)
        };

        if !is_referenced {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV002,
                Severity::Note,
                format!(
                    "Environment variable '{}' declared in contract is not referenced in scanned source code",
                    key_str
                ),
                Some(def.name.clone()),
                vec![def.location.clone()],
                None,
            ));
        }
    }

    // Environment file audit (Rules 3 & 4)
    if let Some(env_defs) = env_definitions {
        let mut env_map: HashMap<&str, &Definition> = HashMap::new();
        for def in env_defs {
            env_map.insert(def.name.as_str(), def);
        }

        // Rule 3: Missing in env (ENV003)
        // Defined in .env.example but absent in provided .env
        for def in &contract.definitions {
            let key_str = def.name.as_str();
            if options.is_ignored(key_str) {
                continue;
            }

            let found_env_def = if options.case_insensitive {
                env_defs
                    .iter()
                    .find(|d| d.name.as_str().eq_ignore_ascii_case(key_str))
            } else {
                env_map.get(key_str).copied()
            };

            if found_env_def.is_none() {
                let (severity, desc) = if def.is_optional {
                    (
                        Severity::Note,
                        format!(
                            "Optional variable '{}' declared in contract is absent from .env",
                            key_str
                        ),
                    )
                } else {
                    (
                        Severity::Error,
                        format!(
                            "Required environment variable '{}' declared in contract is absent from .env",
                            key_str
                        ),
                    )
                };

                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::ENV003,
                    severity,
                    desc,
                    Some(def.name.clone()),
                    vec![def.location.clone()],
                    None,
                ));
            }
        }

        // Rule 4: Empty required value (ENV007)
        // Supplied .env has an empty value for a variable not marked optional
        for env_def in env_defs {
            let key_str = env_def.name.as_str();
            if options.is_ignored(key_str) {
                continue;
            }

            if env_def.value_kind.is_empty() {
                let matching_contract_def = if options.case_insensitive {
                    contract
                        .definitions
                        .iter()
                        .find(|d| d.name.as_str().eq_ignore_ascii_case(key_str))
                } else {
                    contract_map.get(key_str).copied()
                };

                if let Some(contract_def) = matching_contract_def {
                    if !contract_def.is_optional {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticCode::ENV007,
                            Severity::Warning,
                            format!(
                                "Environment variable '{}' in .env has empty value but is not marked optional in contract",
                                key_str
                            ),
                            Some(env_def.name.clone()),
                            vec![env_def.location.clone(), contract_def.location.clone()],
                            None,
                        ));
                    }
                }
            }
        }
    }

    // Rule 6: Leak detection (ENV032)
    // Warn when a @secret variable or likely-secret name has a client prefix or is marked @public
    for def in &contract.definitions {
        let name = def.name.as_str();
        let upper_name = name.to_ascii_uppercase();
        let is_likely_secret = def.annotations.is_secret
            || upper_name.ends_with("_KEY")
            || upper_name.ends_with("_SECRET")
            || upper_name.ends_with("_TOKEN")
            || upper_name.ends_with("_PASSWORD")
            || upper_name.ends_with("_PRIVATE")
            || upper_name.ends_with("_AUTH");

        let has_client_prefix = DEFAULT_PLATFORM_PREFIXES
            .iter()
            .any(|p| upper_name.starts_with(p));

        if is_likely_secret && (def.annotations.is_public || has_client_prefix) {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV032,
                Severity::Warning,
                format!(
                    "Secret leak risk: variable '{}' is designated or named like a secret but has client-exposed prefix or is marked @public",
                    name
                ),
                Some(def.name.clone()),
                vec![def.location.clone()],
                None,
            ));
        }
    }

    // Step 3.3: Strict deterministic sorting: path -> line -> column -> code -> name
    Diagnostic::sort_diagnostics(&mut diagnostics);

    diagnostics
}
