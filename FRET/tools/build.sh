#!/usr/bin/env bash

# Always use the script's directory as the working directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

mkdir -p bin

build() {
    if [ -d "$1" ]; then
        cd "$1" || exit 1
        cargo build --release
        ln -rsf target/release/"$(basename "$1")" ../bin/"$(basename "$1")"
        cd - || exit 1
    else
        echo "Directory $1 does not exist."
    fi
}

build edge_compare
build graph2viz
build input_serde
build number_cruncher
build state2gantt
ln -rsf state2gantt/gantt_driver  bin/gantt_driver
ln -rsf state2gantt/plot_gantt.r  bin/plot_gantt.r