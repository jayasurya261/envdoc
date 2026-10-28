#!/usr/bin/env bash
# Mutation Testing runner using cargo-mutants
set -euo pipefail

echo "==> Running Mutation Testing on EnvDoctor core modules..."
if ! command -v cargo-mutants &> /dev/null; then
    echo "cargo-mutants not found. Install via: cargo install cargo-mutants"
    exit 1
fi

cargo mutants --no-shuffle -v
echo "==> Mutation testing completed successfully."

