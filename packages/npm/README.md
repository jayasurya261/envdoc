# envdoc-cli

[![npm version](https://img.shields.io/npm/v/envdoc-cli.svg)](https://www.npmjs.com/package/envdoc-cli)
[![CI](https://github.com/jayasurya261/envdoc/actions/workflows/ci.yml/badge.svg)](https://github.com/jayasurya261/envdoc/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/jayasurya261/envdoc/blob/master/LICENSE-MIT)

> **Fast, safe-by-construction AST auditor keeping `.env.example` in sync with TypeScript, JavaScript, and Rust source code.**

---

## Why envdoc-cli?

Modern applications suffer from silent environment drift:
1. Developers call `process.env.VAR` or `import.meta.env.VAR` without updating `.env.example`.
2. Deployments or CI builds crash when new teammates pull the repo and encounter missing variables.
3. Obsolete or dead environment variables linger in `.env.example` forever.
4. Traditional linters often leak sensitive secrets in error outputs.

**`envdoc-cli`** audits your code with **native AST static analysis** and a **mathematical zero-secret-leakage guarantee** (secrets are stripped at the tokenizer boundary and never stored in memory).

---

## Zero-Install Quick Start

Run it immediately in any repository without installing:

```bash
npx envdoc-cli
```

Audit your live `.env` file against `.env.example`:
```bash
npx envdoc-cli --env .env
```

Fail CI if undocumented variables or syntax errors are found:
```bash
npx envdoc-cli --fail-on warn
```

---

## Local Installation

Add to your project's development dependencies:

```bash
npm install -D envdoc-cli
# or
pnpm add -D envdoc-cli
# or
yarn add -D envdoc-cli
```

Add an audit script to your `package.json`:
```json
{
  "scripts": {
    "audit:env": "envdoc --fail-on warn"
  }
}
```

Now you can run:
```bash
npm run audit:env
```

---

## Supported Syntax & Patterns

### JavaScript & TypeScript
Supported files: `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.mts`, `.cts`

* **Direct Member Access**: `process.env.DATABASE_URL`
* **Bracket Literals**: `process.env['DATABASE_URL']`
* **Object Destructuring**: `const { API_KEY, PORT } = process.env;`
* **Vite / ESM Import Meta**: `import.meta.env.VITE_API_URL`
* **Dynamic Bracket Access**: `process.env[dynamicKey]` &rarr; Flagged with `ENV010` (honest static analysis)
* **Environment Escape**: `const env = process.env;` &rarr; Flagged with `ENV012`

### Rust
Supported files: `.rs`

* **Std Env**: `std::env::var("VAR")` / `std::env::var_os("VAR")`
* **Imported Env**: `env::var("VAR")`
* **Compile-Time Env**: `env!("VAR")` / `option_env!("VAR")`
* **Dynamic Access**: `std::env::var(dynamic_var)` &rarr; Flagged with `ENV010`

---

## Diagnostic Codes

| Code | Severity | Description |
| :--- | :--- | :--- |
| `ENV001` | Warning | Variable used in code is missing from `.env.example`. |
| `ENV002` | Note | Variable declared in `.env.example` is never referenced in code. |
| `ENV003` | Error | Required variable in contract is missing from active `.env` file. |
| `ENV004` | Warning | Duplicate key in `.env` file. |
| `ENV005` | Error | Malformed syntax in `.env` file. |
| `ENV006` | Warning | Unsupported shell syntax (e.g. `$(cmd)`). |
| `ENV007` | Warning | Required variable has an empty value in active `.env`. |
| `ENV010` | Note | Dynamic bracket access cannot be statically resolved. |
| `ENV011` | Warning | Syntax error in source file. |
| `ENV012` | Note | Full `process.env` object aliased or spread. |
| `ENV020` | Error | Contract file (`.env.example`) was not found. |

---

## CI / GitHub Actions

Add this step to your GitHub Actions workflow:

```yaml
- name: Audit Environment Contract
  run: npx envdoc-cli --fail-on warn
```

---

## License

Dual-licensed under [MIT](https://github.com/jayasurya261/envdoc/blob/master/LICENSE-MIT) OR [Apache-2.0](https://github.com/jayasurya261/envdoc/blob/master/LICENSE-APACHE).
