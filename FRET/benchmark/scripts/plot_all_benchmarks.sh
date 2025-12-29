#!/bin/sh
if [[ -n "$1" ]]; then
  TARGET="$1"
else
  TARGET=$BENCHDIR
fi

# Check if bench.sqlite needs to be updated
if [[ ! -f $TARGET/bench.sqlite || $(find $TARGET/timedump -name '.*[0-9]+\.time' -newer $TARGET/bench.sqlite | wc -l) -gt 0 ]]; then
  number_cruncher -i $TARGET/timedump -o $TARGET/bench.sqlite
fi

Rscript scripts/plot_sqlite.r $TARGET/bench.sqlite $TARGET
Rscript scripts/plot_diffs.r $TARGET/bench.sqlite $TARGET
