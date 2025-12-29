#!/usr/bin/env bash
./sem.sh /tmp/plot reset 20
declare -a PLOTS
COUNT=0
while IFS="" read -r p || [ -n "$p" ];
do
    if [[ -z "$p" ]]; then
        continue
    fi
    PLOTS[$COUNT]="$p"
    COUNT=$((COUNT+1))
    ../../../../state2gantt/driver_sem.sh $p &
done < <(find $BENCHDIR/timedump -maxdepth 2 -type 'f' -iregex '.*icounttrace.ron$')
