use crate::diagnostics::report::Report;
use crate::diagnostics::severity::Severity;
use serde_json::json;

/// Renders report findings in SARIF 2.1.0 format for GitHub Code Scanning.
pub fn render(report: &Report) -> String {
    let runs = vec![json!({
        "tool": {
            "driver": {
                "name": "envdoctor",
                "version": report.tool_version,
                "informationUri": "https://github.com/jayasurya261/envdoc",
                "rules": [
                    {
                        "id": "ENV001",
                        "shortDescription": { "text": "Undocumented environment variable referenced in code" },
                        "defaultConfiguration": { "level": "warning" }
                    },
                    {
                        "id": "ENV002",
                        "shortDescription": { "text": "Unused environment variable defined in contract" },
                        "defaultConfiguration": { "level": "note" }
                    },
                    {
                        "id": "ENV003",
                        "shortDescription": { "text": "Required contract environment variable missing from .env" },
                        "defaultConfiguration": { "level": "error" }
                    },
                    {
                        "id": "ENV004",
                        "shortDescription": { "text": "Duplicate key in environment file" },
                        "defaultConfiguration": { "level": "warning" }
                    },
                    {
                        "id": "ENV005",
                        "shortDescription": { "text": "Malformed line in environment file" },
                        "defaultConfiguration": { "level": "error" }
                    },
                    {
                        "id": "ENV006",
                        "shortDescription": { "text": "Unsupported syntax in environment file" },
                        "defaultConfiguration": { "level": "warning" }
                    },
                    {
                        "id": "ENV007",
                        "shortDescription": { "text": "Empty required value in environment file" },
                        "defaultConfiguration": { "level": "warning" }
                    },
                    {
                        "id": "ENV010",
                        "shortDescription": { "text": "Unresolved dynamic access" },
                        "defaultConfiguration": { "level": "note" }
                    },
                    {
                        "id": "ENV011",
                        "shortDescription": { "text": "Source parser syntax problem" },
                        "defaultConfiguration": { "level": "warning" }
                    },
                    {
                        "id": "ENV012",
                        "shortDescription": { "text": "Environment object aliasing or whole object escape" },
                        "defaultConfiguration": { "level": "note" }
                    },
                    {
                        "id": "ENV030",
                        "shortDescription": { "text": "Active environment value type mismatch against contract" },
                        "defaultConfiguration": { "level": "error" }
                    },
                    {
                        "id": "ENV031",
                        "shortDescription": { "text": "Active environment value enum mismatch against contract" },
                        "defaultConfiguration": { "level": "error" }
                    },
                    {
                        "id": "ENV032",
                        "shortDescription": { "text": "Secret leak risk on client-exposed variable" },
                        "defaultConfiguration": { "level": "warning" }
                    }
                ]
            }
        },
        "results": report.diagnostics.iter().map(|d| {
            let level = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
            };

            let locations = d.locations.iter().map(|loc| {
                json!({
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": loc.path.as_str()
                        },
                        "region": {
                            "startLine": loc.span.line,
                            "startColumn": loc.span.column,
                            "charOffset": loc.span.start_offset,
                            "charLength": loc.span.len()
                        }
                    }
                })
            }).collect::<Vec<_>>();

            json!({
                "ruleId": d.code.as_str(),
                "level": level,
                "message": { "text": d.message },
                "locations": locations
            })
        }).collect::<Vec<_>>()
    })];

    let sarif = json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": runs
    });

    serde_json::to_string_pretty(&sarif).unwrap_or_default()
}
