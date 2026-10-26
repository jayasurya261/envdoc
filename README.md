# EnvDoctor (`envdoc`)

[![Crates.io](https://img.shields.io/crates/v/envdoc.svg)](https://crates.io/crates/envdoc)
[![npm](https://img.shields.io/npm/v/envdoc-cli.svg)](https://www.npmjs.com/package/envdoc-cli)
[![CI](https://github.com/jayasurya261/envdoc/actions/workflows/ci.yml/badge.svg)](https://github.com/jayasurya261/envdoc/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

> **Fast, safe-by-construction AST auditor keeping `.env.example` in sync with Rust, TypeScript, and JavaScript source code.**

---

## The Problem

Modern applications (especially full-stack apps mixing TypeScript and Rust, such as Next.js + Axum or Tauri) suffer from silent configuration drift:
1. Developers introduce calls to `process.env` or `std::env::var` without documenting them in `.env.example`.
2. New teammates or CI pipelines crash because required environment variables are undocumented and missing.
3. Obsolete or dead variables linger in `.env.example` long after their feature code has been removed.
4. Many existing linters leak secret values in error messages or logs, or guess dynamic variables inaccurately.

**EnvDoctor** solves this through honest static analysis, zero-leakage redaction, and multi-language AST scanning in a single pass.

---

## Key Guarantees & Features

* 🛡️ **Safe by Construction (Zero Secret Leakage)**: Secret values are stripped at the tokenizer boundary into classification metadata (`Empty`, `NonEmpty`, `MultiLine`). No raw secret values exist in memory, ASTs, diagnostics, or report payloads.
* ⚡ **Multi-Language in One Pass**: Native AST scanning for **Rust** (via `syn`) and **JavaScript / TypeScript** (via `oxc`) simultaneously without external node/npm runtimes.
* 🔍 **Honest Static Analysis**: Accurately distinguishes direct member accesses, destructuring, and computed bracket accesses. Unresolved dynamic accesses are explicitly flagged (`ENV010`), never guessed or silently ignored.
* 🤖 **CI-First Design**: Deterministic diagnostic ordering, stable exit codes (`0`, `1`, `2`, `3`), and first-class SARIF / JSON output for automated CI gating and GitHub Code Scanning.

---

## Installation

### Via Cargo (Rust)
```bash
cargo install envdoc
```

### Via npm (Node.js / TypeScript)
```bash
npm install -D envdoc-cli
# or run zero-install via npx:
npx envdoc-cli
```

This installs the binary command as `envdoctor` (and `envdoc`). Verify installation:

```bash
envdoctor --version
```

---

## Quick Start

### 1. Basic Audit
Run `envdoctor` in your project root. It discovers `.env.example` and audits all source files:

```bash
envdoctor
```

### 2. Verify an Active Environment File
Ensure that your live `.env` file defines all required variables from your contract:

```bash
envdoctor --env .env
```

### 3. Fail on Warnings in CI
Configure your CI pipeline to exit with a non-zero code if undocumented variables are found:

```bash
envdoctor --fail-on warn
```

### 4. Filter Severity Output
Hide informational notes and only show warnings and errors:

```bash
envdoctor --min-severity warn
```

### 5. Machine-Readable & SARIF Output
Generate JSON reports or SARIF for GitHub Security Code Scanning:

```bash
# JSON output
envdoctor --format json

# SARIF output
envdoctor --format sarif > results.sarif
```

---

## Supported Languages & Syntax Patterns

### JavaScript & TypeScript
Files: `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.mts`, `.cts`

| Pattern | Example | Evidence Kind |
| :--- | :--- | :--- |
| **Direct Member** | `process.env.API_KEY` | `direct-member` |
| **Bracket Literal** | `process.env['API_KEY']` | `direct-member` |
| **Object Destructuring** | `const { API_KEY, PORT } = process.env;` | `destructured` |
| **Vite / ESM Import Meta** | `import.meta.env.VITE_URL` | `direct-member` |
| **Dynamic Access** | `process.env[dynamicKey]` | `dynamic-unresolved` (`ENV010`) |
| **Environment Escape** | `const env = process.env;` or `{ ...process.env }` | `whole-object-escape` (`ENV012`) |

### Rust
Files: `.rs`

| Pattern | Example | Evidence Kind |
| :--- | :--- | :--- |
| **Standard Library Var** | `std::env::var("DATABASE_URL")` | `direct-member` |
| **Imported Var** | `env::var("DATABASE_URL")` | `direct-member` |
| **Compile-Time Env Macro** | `env!("RUST_LOG")` | `direct-member` |
| **Optional Env Macro** | `option_env!("FEATURE_FLAG")` | `direct-member` |
| **Dynamic Access** | `std::env::var(dynamic_var)` | `dynamic-unresolved` (`ENV010`) |

---

## Diagnostic Codes

| Code | Severity | Name | Description |
| :--- | :--- | :--- | :--- |
| `ENV001` | Warning | `undocumented-reference` | Variable used in code is missing from `.env.example`. |
| `ENV002` | Note | `unused-definition` | Variable declared in `.env.example` is never referenced in scanned code. |
| `ENV003` | Error | `missing-in-env` | Required variable defined in contract is missing from active `.env` file. |
| `ENV004` | Warning | `duplicate-key` | Key defined multiple times in `.env` file. |
| `ENV005` | Error | `malformed-line` | Syntax error in `.env` line. |
| `ENV006` | Warning | `unsupported-syntax` | Valid shell construct unsupported by basic dotenv dialect (e.g. `$(cmd)`). |
| `ENV007` | Warning | `empty-required-value` | Required variable has empty value in active `.env` file. |
| `ENV010` | Note | `unresolved-dynamic-access` | Bracket access cannot be statically resolved. |
| `ENV011` | Warning | `source-parse-problem` | Syntax error while parsing source file. |
| `ENV012` | Note | `environment-escapes` | Entire `process.env` object aliased or spread. |
| `ENV020` | Error | `contract-missing` | Contract file (`.env.example`) was not found. |
| `ENV021` | Error | `contract-empty` | Contract file contains no variable definitions. |
| `IO001` | Error | `file-not-found` | Specified file could not be read. |
| `IO002` | Error | `invalid-utf8` | File contains non-UTF-8 bytes. |
| `IO003` | Error | `path-not-a-file` | Target path is a directory, not a file. |
| `IO010` | Error | `resource-limit-exceeded` | File size, file count, or directory depth exceeded bounds. |

---

## Exit Codes

EnvDoctor uses explicit, stable exit codes for CI reliability:

* `0`: Audit succeeded cleanly (or violations were below `--fail-on` threshold).
* `1`: Contract or rule violations encountered (e.g. undocumented variables, missing required values).
* `2`: Contract file missing or empty (`ENV020`, `ENV021`).
* `3`: Fatal I/O or execution error (unreadable files, resource limits exceeded).

---

## GitHub Actions Example

```yaml
name: Environment Contract Audit

on:
  push:
    branches: [main, master]
  pull_request:

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install EnvDoctor
        run: cargo install envdoc

      - name: Audit Environment Contract
        run: envdoctor --fail-on warn
```

---

## License

Licensed under either of:
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
* MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
