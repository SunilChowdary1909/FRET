#!/bin/sh

TEST_KERNEL=../benchmark/build/waters_seq_full.elf
TEST_SYMBOLS=../benchmark/target_symbols.csv
DEF_ARGS="-k $TEST_KERNEL -c $TEST_SYMBOLS -n ./dump/test"

# cargo build --no-default-features --features std,snapshot_restore,singlecore,feed_afl,observer_hitcounts

# Test basic fuzzing loop
# ../target/debug/fret $DEF_ARGS -tar fuzz -t 10 -s 123

# Test reprodcibility
rm -f ./dump/test.time
../target/debug/fret $DEF_ARGS -tr showmap -i ./waters.case.test
if [[ $(cut -d, -f1 ./dump/test.time) != $(cut -d, -f1 ./waters.time.test) ]]; then echo "Not reproducible!" && exit 1; else echo "Reproducible"; fi

# Test state dump
# cargo build --no-default-features --features std,snapshot_restore,singlecore,feed_afl,observer_hitcounts,systemstate
if [[ -n "$(diff -q demo.example.state.ron dump/demo.trace.ron)" ]]; then echo "State not reproducible!"; else echo "State Reproducible"; fi

# Test abb traces
# cargo build --no-default-features --features std,snapshot_restore,singlecore,feed_afl,observer_hitcounts,systemstate,trace_abbs
if [[ -n "$(diff -q demo.example.abb.ron dump/demo.trace.ron)" ]]; then echo "ABB not reproducible!"; else echo "ABB Reproducible"; fi

# ../target/debug/fret -k ../benchmark/build/minimal.elf -c ../benchmark/target_symbols.csv -n ./dump/minimal -tar fuzz -t 20 -s 123
# ../target/debug/fret -k ../benchmark/build/minimal.elf -c ../benchmark/target_symbols.csv -n ./dump/minimal_worst -tr showmap -i ./dump/minimal.case

# Test fuzzing using systemtraces
cargo build --no-default-features --features std,snapshot_restore,singlecore,config_stg

../target/debug/fret -k ../benchmark/build/waters_seq_full.elf -c ../benchmark/target_symbols.csv -n ./dump/waters -tar fuzz -t 10 -s 123
