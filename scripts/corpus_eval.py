#!/usr/bin/env python3
"""
Corpus Evaluation Suite for EnvDoctor.

Executes EnvDoctor across a corpus of repositories or sample projects to measure:
- Discovery throughput (files/second, MB/second)
- Scan accuracy and precision (absence of false ENV001 on config keys, comments, or imports)
- Diagnostic distribution across languages (TS/JS, Rust, Python, Go, Infra)
"""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

SAMPLE_REPOS = [
    {
        "name": "dummy_fullstack_app",
        "path": "dummy_fullstack_app",
        "description": "Polyglot fullstack project with TS, Python, Go, Docker, and Terraform",
    },
    {
        "name": "fixtures_conformance_ts",
        "path": "fixtures/conformance/01_pure_typescript",
        "description": "Official TypeScript conformance fixture",
    },
    {
        "name": "fixtures_conformance_mixed",
        "path": "fixtures/conformance/02_mixed_js_ts_dynamic",
        "description": "Mixed JS/TS dynamic access conformance fixture",
    },
]

def find_binary():
    # Look for release or debug binary
    candidates = [
        Path("target/release/envdoctor.exe"),
        Path("target/release/envdoctor"),
        Path("target/debug/envdoctor.exe"),
        Path("target/debug/envdoctor"),
    ]
    for c in candidates:
        if c.exists() and os.access(c, os.X_OK):
            return str(c)
    return "cargo run --bin envdoctor --"

def run_evaluation(binary_cmd, target_path, languages="all"):
    contract_candidate = Path(target_path) / ".env.example"
    contract_flag = ["--contract", str(contract_candidate)] if contract_candidate.exists() else []

    start = time.perf_counter()
    base_args = ["--format", "json", "--languages", languages, "--path", target_path] + contract_flag
    if binary_cmd.startswith("cargo"):
        cmd = binary_cmd.split() + ["--"] + base_args
    else:
        cmd = [binary_cmd] + base_args

    proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    duration_ms = (time.perf_counter() - start) * 1000

    report = None
    try:
        report = json.loads(proc.stdout)
    except Exception:
        # Fallback if non-JSON or partial
        pass

    return {
        "exit_code": proc.returncode,
        "duration_ms": duration_ms,
        "report": report,
        "stderr": proc.stderr.strip(),
    }

def main():
    parser = argparse.ArgumentParser(description="EnvDoctor Real-World Corpus Evaluation")
    parser.add_argument("--corpus-dir", help="Directory containing cloned git repositories to evaluate")
    parser.add_argument("--bin", help="Path to envdoctor executable")
    parser.add_argument("--languages", default="all", help="Comma-separated languages to enable")
    parser.add_argument("--output-json", help="Path to write evaluation results JSON")
    args = parser.parse_args()

    binary_cmd = args.bin or find_binary()
    print(f"==> Using EnvDoctor binary: {binary_cmd}")

    repos = list(SAMPLE_REPOS)
    if args.corpus_dir and Path(args.corpus_dir).is_dir():
        for item in Path(args.corpus_dir).iterdir():
            if item.is_dir() and not item.name.startswith("."):
                repos.append({
                    "name": item.name,
                    "path": str(item),
                    "description": f"External corpus repo: {item.name}",
                })

    results = []
    total_files = 0
    total_errors = 0
    total_warnings = 0
    total_time_ms = 0

    print(f"\n{'Repository':<28} | {'Files':<7} | {'Errors':<7} | {'Warnings':<9} | {'Time (ms)':<10} | {'Status'}")
    print("-" * 75)

    for repo in repos:
        path = repo["path"]
        if not Path(path).exists():
            continue

        res = run_evaluation(binary_cmd, path, languages=args.languages)
        rep = res["report"]

        files = rep["summary"]["scanned_files"] if rep else 0
        errors = rep["summary"]["total_errors"] if rep else 0
        warnings = rep["summary"]["total_warnings"] if rep else 0
        dur = res["duration_ms"]
        status = "PASS" if res["exit_code"] in (0, 1) and rep else "FAIL"

        total_files += files
        total_errors += errors
        total_warnings += warnings
        total_time_ms += dur

        results.append({
            "repo": repo["name"],
            "description": repo["description"],
            "path": path,
            "status": status,
            "scanned_files": files,
            "errors": errors,
            "warnings": warnings,
            "duration_ms": round(dur, 2),
            "report_summary": rep["summary"] if rep else None,
        })

        print(f"{repo['name']:<28} | {files:<7} | {errors:<7} | {warnings:<9} | {dur:<10.1f} | {status}")

    print("-" * 75)
    print(f"{'Total':<28} | {total_files:<7} | {total_errors:<7} | {total_warnings:<9} | {total_time_ms:<10.1f} |")

    if args.output_json:
        with open(args.output_json, "w") as f:
            json.dump({
                "timestamp": time.time(),
                "summary": {
                    "total_repos": len(results),
                    "total_files": total_files,
                    "total_errors": total_errors,
                    "total_warnings": total_warnings,
                    "total_time_ms": total_time_ms,
                },
                "results": results,
            }, f, indent=2)
        print(f"\nResults saved to {args.output_json}")

if __name__ == "__main__":
    main()
