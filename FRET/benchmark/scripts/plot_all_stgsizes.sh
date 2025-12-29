get_max_nodecount () {
    rm -f sizecomp && for sizefile in $BENCHDIR/timedump/**/$1*.stgsize;do echo "$(tail -n 1 $sizefile),${sizefile}" >> sizecomp; done; sort -n sizecomp | tail -n 1
}

get_largest_files () {
    T=$(get_max_nodecount $1)
    echo $T | cut -d',' -f6
}

perform () {
    T=$(get_max_nodecount $1)
    echo $T | cut -d',' -f6
    echo $T | cut -d',' -f6 | xargs -I {} ./plot_stgsize.r {}
    mv "$(echo $T | cut -d',' -f6 | xargs -I {} basename -s .stgsize {})_nodes.png" $1_nodes.png
}

# perform copter
# perform release
# perform waters
A=$(get_largest_files polycopter_seq_dataflow_full) 
B=$(get_largest_files release_seq_full) 
C=$(get_largest_files waters_seq_full) 
# A_="$(echo $A | sed 's/polycopter_seq_dataflow_full/UAV w. hid. com./')"
# B_="$(echo $B | sed 's/release_seq_full/Async. rel./')"
# C_="$(echo $C | sed 's/waters_seq_full/Waters ind. ch./')"
A_="UAV"
B_="Async. rel."
C_="Waters ind. ch."
echo $A_ $B_ $C_
cp $A "$A_"
cp $B "$B_"
cp $C "$C_"
./plot_stgsize_multi.r "$A_" "$B_" "$C_"