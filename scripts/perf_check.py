#!/usr/bin/env python3
"""
CI Performance Regression Gate for EnvDoctor.

Runs end-to-end audit scans across a generated synthetic codebase of 1,000 files
and checks that p95 execution latency remains strictly within the acceptable threshold.
"""

import os
import sys
import time
import subprocess
import tempfile
import statistics

NUM_FILES = 1000
ACCEPTABLE_MAX_P95_SECONDS = 3.0  # 3 seconds for 1,000 files in parallel with prefiltering

def create_synthetic_workspace(temp_dir):
    src_dir = os.path.join(temp_dir, "src")
    os.makedirs(src_dir, exist_ok=True)

    contract_path = os.path.join(temp_dir, ".env.example")
    with open(contract_path, "w", encoding="utf-8") as f:
        f.write("PORT=\nDATABASE_URL=\nAPI_KEY=\nREDIS_URL=\n")

    for i in range(NUM_FILES):
        file_path = os.path.join(src_dir, f"module_{i:04d}.ts")
        with open(file_path, "w", encoding="utf-8") as f:
            if i % 20 == 0:
                f.write(f"// Module {i} accesses env\n")
                f.write("export const port = process.env.PORT;\n")
                f.write("export const db = process.env.DATABASE_URL;\n")
            else:
                f.write(f"// Clean module {i}\n")
                f.write(f"export function compute_{i}(x: number): number {{ return x * {i}; }}\n")

def run_benchmark():
    print(f"Creating synthetic workspace with {NUM_FILES} files...")
    with tempfile.TemporaryDirectory() as temp_dir:
        create_synthetic_workspace(temp_dir)

        durations = []
        # Run 5 warm iterations
        print("Executing benchmark runs...")
        for run_idx in range(5):
            start = time.perf_counter()
            res = subprocess.run(
                ["cargo", "run", "--bin", "envdoctor", "--", "--path", temp_dir, "--contract", os.path.join(temp_dir, ".env.example"), "--format", "json"],
                capture_output=True,
                text=True
            )
            duration = time.perf_counter() - start
            durations.append(duration)
            print(f"  Run {run_idx + 1}: {duration:.3f}s (exit code {res.returncode})")

        p95 = statistics.quantiles(durations, n=100)[94] if len(durations) >= 5 else max(durations)
        print(f"\nBenchmark Results across {NUM_FILES} files:")
        print(f"  Min: {min(durations):.3f}s")
        print(f"  Avg: {statistics.mean(durations):.3f}s")
        print(f"  p95: {p95:.3f}s")

        if p95 > ACCEPTABLE_MAX_P95_SECONDS:
            print(f"FAILED: p95 latency ({p95:.3f}s) exceeded budget ({ACCEPTABLE_MAX_P95_SECONDS:.3f}s)!")
            sys.exit(1)
        else:
            print(f"PASSED: Performance is well within budget ({p95:.3f}s <= {ACCEPTABLE_MAX_P95_SECONDS:.3f}s).")

if __name__ == "__main__":
    run_benchmark()

