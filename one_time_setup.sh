#!/usr/bin/env bash

if  [[ -z "$INSIDE_DEVSHELL" ]]; then
  echo "This script should be run inside a nix-shell. Run 'nix develop' or 'nix-shell' first."
  exit 1
fi

# Always use the script's directory as the working directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Ensure that all sources are up-to-date
#git submodule update --init --recursive

# The central directory for the benchmarks
cd LibAFL/fuzzers/FRET/benchmark

# one-time setup
# build QEMU for the first time
snakemake -c 1 rebuild_qemu
# Build kelper tools to aid the analysis of the benchmarks
snakemake -c 1 build_tools
# Build the kernels for the benchmarks
snakemake -c 1 build_kernels

cd -