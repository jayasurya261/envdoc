# EnvDoctor

> **Environment Contract Checker**: A read-only, AST-based, safe-by-construction, CI-grade auditor keeping `.env.example` in sync with Rust & TypeScript source code.

## Key Principles

- **Honest Static Analysis**: Scans direct property accesses, destructuring, and brackets without guessing or silently skipping dynamic accesses.
- **Safe by Construction**: Secret values are classified into metadata kinds (`Empty`, `NonEmpty`, `MultiLine`) at the tokenizer boundary. No secret strings are retained in the AST or Diagnostic types.
- **CI-First Contract**: Stable, non-reusable diagnostic codes (`ENV001`–`ENV021`, `IO001`–`IO010`), versioned JSON reports, deterministic sorting, and explicit exit codes (`0`, `1`, `2`, `3`).
- **Multi-Language in One Pass**: Native AST scanning for JavaScript/TypeScript and Rust with workspace awareness.

## Project Layout

- `crates/envdoctor`: Primary library and CLI crate.
  - `model`: Core data types (`RelPath`, `Span`, `EnvName`, `ValueKind`, `EvidenceKind`, `Definition`, `Reference`).
  - `redaction`: Zero-leakage redaction wrappers.

