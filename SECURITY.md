# Security Policy

## Supported Versions

Security updates and patches are provided for the current release version of `envdoc` (crates.io) and `envdoc-cli` (npm).

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

---

## Reporting a Vulnerability

The EnvDoctor maintainers take security and privacy seriously. Because EnvDoctor processes sensitive environment configuration files (`.env`), we prioritize the prevention of secret leakage, memory disclosure, and denial-of-service vulnerabilities.

If you believe you have discovered a vulnerability in EnvDoctor:

1. **Do not open a public issue.**
2. Report the vulnerability privately via **GitHub Security Advisories** at:
   `https://github.com/jayasurya261/envdoc/security/advisories/new`
3. Alternatively, email the maintainer directly at:
   `jayasurya261@gmail.com` with the subject `[SECURITY] EnvDoctor Vulnerability Report`.

### What to Include in Your Report
- A detailed description of the vulnerability.
- Proof-of-concept (PoC) code or minimal reproducible project files.
- Impact assessment (e.g. memory disclosure, secret leakage in logs, CPU/memory resource exhaustion).
- Any proposed remediation or patches.

### Response Timeline
- **Initial Acknowledgment**: Within 48 hours of receipt.
- **Triage & Assessment**: Within 7 business days.
- **Remediation & Patch Release**: Coordinated with the reporter, targeted within 30 days.
- **Public Disclosure**: Follows coordinated disclosure after an advisory and patched release are available.

---

## Security Architecture & Design Guarantees

EnvDoctor follows a strict **Safe-by-Construction** security architecture:

1. **Zero Secret Leakage by Design**:
   - Environment values parsed from `.env` files are stripped into classification tokens (`ValueKind::Empty`, `ValueKind::NonEmpty`, `ValueKind::MultiLine`).
   - Secret bytes are held in memory-protected, zeroizing buffers (`zeroize`) and wiped from RAM upon drop.
   - Core data models (`Definition`, `Reference`, `Diagnostic`, `Report`) do not contain secret value fields, preventing accidental serialization or display.
2. **Pure Core Invariant**:
   - Compiler lints (`clippy::disallowed_methods`) prohibit filesystem I/O across all parsing, AST scanning, and rule evaluation modules.
   - Analysis executes entirely in-memory on validated buffers.
3. **Resource Exhaustion Mitigations**:
   - Bounded file read limits (`--max-file-size`, default 2 MB).
   - Candidate file discovery caps (`--max-files`, default 10,000).
   - Directory recursion depth limits (`--max-depth`, default 30).
   - AST parser stack isolation and panic boundaries catching deeply nested inputs without process abortion.
   - Symlink traversal disabled by default to prevent directory loop attacks.
4. **Zero Unsafe Code**:
   - The crate is compiled with `#![forbid(unsafe_code)]`.

