#!/usr/bin/env bash
find $1 -type 'f' -iname "${2}#*.log" | while IFS="" read -r p || [ -n "$p" ]
do
    LINE=$(tail -n 100 $p | grep -io "run time: .* corpus: [0-9]*" | tail -n 1)
    echo $p: $LINE
    LINE=$(grep -i "interesting corpus elements" $p | tail -n 1)
    echo $p: $LINE
done
