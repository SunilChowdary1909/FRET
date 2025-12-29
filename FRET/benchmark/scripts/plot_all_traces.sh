#!/usr/bin/env bash
declare -a PLOTS
COUNT=0
while IFS="" read -r p || [ -n "$p" ];
do
    if [[ -z "$p" ]]; then
        continue
    fi
    N="$(dirname "$p")/$(basename -s .case "$p")"
    T="${N}_case.trace.ron"
    P="${N}_case"
    H="${N}_case.jobs.html"
    echo "$COUNT $p -> $H"
    IFS=" "
    # PLOTS+=("$H")
    PLOTS[$COUNT]="$H"
    COUNT=$((COUNT+1))

    # if [ ! -f "$T" ]; then
    #     snakemake -c1 "$T"
    # fi
    # if [ ! -f "$P.html" ]; then
    #     ~/code/FRET/state2gantt/driver.sh "$T"
    # fi
done < <(find $BENCHDIR/timedump -maxdepth 2 -type 'f' -iregex '.*[0-9]+\.case')

echo "${PLOTS[@]}"
snakemake -c 20 --rerun-incomplete --keep-incomplete "${PLOTS[@]}"
