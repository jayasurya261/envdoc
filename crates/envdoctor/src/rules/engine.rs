use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::model::definition::Definition;
use crate::model::reference::Reference;
use crate::model::span::Location;
use crate::rules::contract::Contract;
use std::collections::{HashMap, HashSet};

/// Options for configuring contract evaluation.
#[derive(Debug, Clone, Default)]
pub struct AuditOptions {
    /// Environment variable keys to ignore during audit (e.g. `NODE_ENV`, `PATH`).
    pub ignore_keys: HashSet<String>,
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
        if !options.ignore_keys.contains(r.name.as_str()) {
            ref_map.entry(r.name.as_str()).or_default().push(r);
        }
    }

    // Rule 1: Undocumented reference (ENV001)
    // Environment variable accessed in source code but absent from .env.example
    for r in references {
        let key_str = r.name.as_str();
        if options.ignore_keys.contains(key_str) {
            continue;
        }

        if !contract_map.contains_key(key_str) {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV001,
                Severity::Warning,
                format!(
                    "Environment variable '{}' referenced in source code is not documented in contract '{}'",
                    key_str, contract.path
                ),
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
        if options.ignore_keys.contains(key_str) {
            continue;
        }

        if !ref_map.contains_key(key_str) {
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
            if options.ignore_keys.contains(key_str) {
                continue;
            }

            if !env_map.contains_key(key_str) {
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
            if options.ignore_keys.contains(key_str) {
                continue;
            }

            if env_def.value_kind.is_empty() {
                if let Some(contract_def) = contract_map.get(key_str) {
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

    // Step 3.3: Strict deterministic sorting: path -> line -> column -> code -> name
    Diagnostic::sort_diagnostics(&mut diagnostics);

    diagnostics
}
