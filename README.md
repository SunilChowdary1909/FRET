# FRET
## Structure
* `git submodule update --init`
* LibAFL-based fuzzer under `LibAFL/fuzzers/FRET`
* FreeRTOS demos under `FreeRTOS/FreeRTOS/Demo/CORTEX_M3_MPS2_QEMU_GCC`
* QEMU instrumentation under `qemu-libafl-bridge`
## HowTo
### Development environment using nix
Use `nix develop` or `nix-shell` to enter a shell with all required tools.
### Development environment using podman/docker
If you don't have nix installed, you can use it though a container.
See Docker/README.md.
### Potential Issues
If you encounter errors where a temporary directory is not found, use `mkdir -p $TMPDIR`
### Build FRET
```sh
cd LibAFL/fuzzers/FRET
# First time and after changes to QEMU
sh -c "unset CUSTOM_QEMU_NO_BUILD CUSTOM_QEMU_NO_CONFIGURE && cargo build"
# Afterwards, simply use
cargo build
```
### Build additional tools
```sh
LibAFL/fuzzers/FRET/tools/build.sh
```
### Build FreeRTOS Demos
```sh
cd LibAFL/fuzzers/FRET/benchmark
sh build_all_demos.sh
# see LibAFL/fuzzers/FRET/benchmark/build
```
### Example usage
* Build the demos and additional tools first
```sh
cd LibAFL/fuzzers/FRET
# Help for arguments
cargo run -- --help
# Example
export DUMP=$(mktemp -d)
dd if=/dev/random of=$DUMP/input bs=8K count=1
# fuzz for 10 seconds
cargo run -- -k benchmark/build/waters_seq_full.elf -c benchmark/target_symbols.csv -n $DUMP/output -tag fuzz -t 10 --seed 123456
# Produce a trace for the worst case found
cargo run -- -k benchmark/build/waters_seq_full.elf -c benchmark/target_symbols.csv -n $DUMP/show -tr showmap -i $DUMP/output.case
# plot the result
../../../state2gantt/driver.sh $DUMP/show.trace.ron
# view the gantt chart
open $DUMP/show_job.html
```
### Perform canned benchmarks
* Build the demos and additional tools first
* Select a benchmark set in `LibAFL/fuzzers/FRET/benchmark/Snakefile`
* Hardware Requirements:
    - Recommendation: 512GiB of RAM with 64 physical cores
    - About 8GB of RAM per Job on average are required to prevent OOMs
    - The set used for the paper consists of ~270 Jobs, so you will need about five day to reproduce the results
```sh
# $BENCHDIR
cd LibAFL/fuzzers/FRET/benchmark
# optional
export BENCHDIR="eval_$(date -I)"
# Reproduce the evals in the paper e.g.
snakemake --cores 64 eval_bytes eval_int eval_full waters_multi
# plot the resutls
snakemake -c20 plot_benchmarks
# See images in $BENCHDIR
snakemake -c20 plot_traces
# See HTML files in $BENCHDIR/timedump/*/ for traces of the worst cases
```