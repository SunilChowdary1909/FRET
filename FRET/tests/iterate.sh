#!/bin/sh
../../../../input_serde/target/debug/input_serde -i edit -c "$1" -f case > test.case
../target/debug/fret -k "$2" -c ../benchmark/target_symbols.csv -n ./dump/test -targ -s "$3" showmap -i ./test.case
../../../../state2gantt/driver.sh dump/test.trace.ron $4