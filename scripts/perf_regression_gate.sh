#!/usr/bin/env bash
set -euo pipefail

echo "=== EnvDoctor Performance Regression Gate ==="
python3 scripts/perf_check.py

