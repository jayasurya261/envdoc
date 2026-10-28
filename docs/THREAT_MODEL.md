# EnvDoctor Threat Model & Security Architecture

**Status:** Active  
**Version:** 1.0.0  
**Target Audience:** Security Engineers, DevSecOps, Enterprise Auditors  

---

## 1. Executive Summary

EnvDoctor is a read-only, AST-based static analysis engine designed to verify that environment variable contracts (`.env.example`) match actual references across source code (TypeScript, JavaScript, Rust, Python, Go, and Infrastructure configurations).

Because EnvDoctor processes sensitive files (including local `.env` files containing production credentials, database URIs, and private API keys) and untrusted codebases in CI runners, security is guaranteed **by construction** rather than convention.

This document details:
1. The **Security Invariants & Guarantees** enforced by EnvDoctor.
2. The **Explicit Non-Guarantees & Out-of-Scope Risks**.
3. Cryptographic protection against cache poisoning.
4. Supply chain and dependency isolation boundaries.

---

## 2. Security Invariants (What EnvDoctor Guarantees)

### 2.1 Safe by Construction: Zero Secret Leakage
- **No Secret Value Emission:** Unredacted secret values never appear in stdout, stderr, human reports, JSON reports, SARIF reports, or JUnit XML files.
- **Forbidden Serialization & Display:** The internal value container `RawEnvValue`:
  - DOES NOT implement `std::fmt::Display` (preventing accidental formatting to stdout/stderr).
  - DOES NOT implement `std::fmt::Debug` (preventing accidental logging via `dbg!()`, `eprintln!()`, or tracing).
  - DOES NOT implement `serde::Serialize` (preventing leakage into JSON/SARIF/JUnit payloads).
- **Compile-Time Enforcement:** Unit and conformance tests verify via `trybuild` that any code attempting to format, log, or serialize a raw environment value fails to compile at build time.
- **Volatile Heap Scrubbing:** Sensitive file buffers and unquoted/quoted string values are stored in `zeroize::Zeroizing<Vec<u8>>` containers that execute volatile zeroing memory wipes immediately upon falling out of scope.

### 2.2 Static AST Only: Zero Code Execution
- **Untrusted Files:** EnvDoctor parses files purely as static text and AST tokens. It **never executes** scanned code.
- **Command Substitution Rejection:** Syntax patterns such as `$(command)` or `` `command` `` in `.env` files are rejected statically as diagnostic `ENV006` (`UnsupportedSyntax`) and are never evaluated or spawned as subprocesses.
- **Resource Exhaustion Shields:**
  - File reading is strictly bounded by `max_file_size` (default: 2 MB) and `max_total_bytes` (default: 100 MB).
  - Individual line lengths are capped by `max_line_length` (default: 65,536 bytes) to defeat catastrophic regex back-tracking and memory amplification attacks.
  - Rayon worker threads run within panic boundaries (`std::panic::catch_unwind`) with 4MB dedicated stack sizes, protecting against stack-overflow crashes from adversarial deeply-nested code.

### 2.3 Cache Integrity & Anti-Poisoning
- **Cryptographic BLAKE3 Hashing:** Cache keys (`.envdoctor-cache`) utilize 256-bit BLAKE3 cryptographic hashes.
- **Composite Cache Invalidation:** The cache key is composed of:
  $$\text{Key} = \text{BLAKE3}\Big(\text{FileContent} \,\|\, \text{ToolVersion} \,\|\, \text{ConfigHash} \,\|\, \text{EnabledLanguages}\Big)$$
- **CI Poisoning Mitigation:** Malicious pull requests cannot manipulate non-cryptographic hashes (e.g. FNV-1a) to craft collision payloads that hide findings or trick subsequent clean CI runs.

### 2.4 Strict Memory Safety (`#![forbid(unsafe_code)]`)
- The entire core codebase enforces `#![forbid(unsafe_code)]`.
- All memory allocations, string parsers, and data structures are 100% safe Rust.

---

## 3. Explicit Non-Guarantees (Out of Scope)

To maintain an honest and realistic security posture, the following threat vectors are explicitly out of scope:

### 3.1 Kernel / Root / Ptrace Memory Introspection
If an attacker possesses `root`, `CAP_SYS_PTRACE`, administrative access, or physical debugger hardware on the host executing EnvDoctor, they can inspect `/proc/<pid>/mem` while the `.env` file is being loaded from the operating system filesystem. EnvDoctor minimizes the secret lifetime in memory with `Zeroizing`, but cannot defend against a compromised operating system kernel.

### 3.2 Operating System Swap and Core Dumps
If the host operating system swaps memory pages to unencrypted swap disks or creates unredacted crash core dumps on abnormal process termination (e.g. `SIGKILL` or hardware power failure), raw page contents may be persisted to disk. Operating system disk encryption (e.g. LUKS, BitLocker) is required to mitigate this risk.

### 3.3 Dynamic Application Runtime Introspection
EnvDoctor is a static analyzer. It reliably identifies static references (`process.env.DATABASE_URL`, `os.getenv("PORT")`, `viper.BindEnv`, etc.). When dynamic bracket access is detected (e.g. `process.env[computeKey()]` or `getattr(os.environ, key)`), EnvDoctor flags the location with `ENV010` (`DynamicAccessUnresolved`), but cannot evaluate what string will be computed at application runtime.

### 3.4 Pre-existing File Permissions
EnvDoctor audits environment contracts; it does not manage filesystem access control lists (ACLs) or file permissions. If a project inadvertently sets world-readable permissions (`chmod 777 .env`) or commits `.env` to a public git history, EnvDoctor will flag the presence of undocumented variables, but cannot alter host filesystem security.

---

## 4. Supply Chain & Dependency Hygiene

- **Audited Dependencies:** All dependencies are audited via `cargo-audit` against the RustSec Advisory Database and verified against `cargo-deny` license and ban policies.
- **Permissive Licensing:** Zero copyleft (GPL) dependencies; only permissive MIT, Apache-2.0, BSD-3-Clause, and Unicode-DFS-2016 licenses are allowed.
- **Isolated Parsing:** Language parsers (e.g. `oxc_parser`, `ruff_python_parser`) are pure safe Rust with no C/C++ compiler or shared object link-time dependencies.

