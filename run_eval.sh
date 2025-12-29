#!/usr/bin/env bash

# Configuration
export CORES=${CORES:-64} # Number of physical cores
export RUNTIME=${RUNTIME:-86400} # 24 hours in seconds
export TARGET_REPLICA_NUMBER=${TARGET_REPLICA_NUMBER:-12}
export RANDOM_REPLICA_NUMBER=${RANDOM_REPLICA_NUMBER:-3}
export MULTIJOB_REPLICA_NUMBER=${MULTIJOB_REPLICA_NUMBER:-3}

if [[ -z "$INSIDE_DEVSHELL" ]]; then
  echo "This script should be run inside a nix-shell. Run 'nix develop' or 'nix-shell' first."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

cd LibAFL/fuzzers/FRET/benchmark

export BENCHDIR="${BENCHDIR:-eval_$(date -I)}"
# prepare all fuzzer configurations
snakemake --keep-incomplete --cores $CORES all_bins
# Run the eval examples from the paper (eval_bytes = Fig. 3, eval_int = Fig. 4, eval_full = Fig. 5, waters_multi = Fig. 6)
snakemake --keep-incomplete --cores $CORES eval_bytes eval_int eval_full waters_multi
# plot the resutls
rm -f $BENCHDIR/bench.sqlite
snakemake --keep-incomplete --cores $CORES plot_benchmarks
# See images in $BENCHDIR
snakemake --keep-incomplete --cores $CORES plot_traces
# See HTML files in $BENCHDIR/timedump/*/ for traces of the worst cases

cd -