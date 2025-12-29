#!/usr/bin/env bash
export INSERT_WC=${2:-0}
export BUILD_DIR=${1:-build}
mkdir -p $BUILD_DIR

build () {
    make -C ../../../../FreeRTOS/FreeRTOS/Demo/CORTEX_M3_MPS2_QEMU_GCC clean && make -C ../../../../FreeRTOS/FreeRTOS/Demo/CORTEX_M3_MPS2_QEMU_GCC $1=1 IGNORE_INTERRUPTS=$IGNORE_INTERRUPTS IGNORE_BYTES=$IGNORE_BYTES IGNORE_INTERNAL_STATE=$IGNORE_INTERNAL_STATE INSERT_WC=$INSERT_WC $EXTRA_MAKE_ARGS
    cp ../../../../FreeRTOS/FreeRTOS/Demo/CORTEX_M3_MPS2_QEMU_GCC/build/RTOSDemo.axf $BUILD_DIR/$(echo $1 | cut -d_ -f1 | tr '[:upper:]' '[:lower:]')$EXTRA_NAME_SUFFIX$2.elf
}

# OSEK/RTA_OS build function for AURIX TC4x demos
build_osek () {
    make -C ../../../../OSEKOS/Demo/TRICORE_TC4x_QEMU_GCC clean && make -C ../../../../OSEKOS/Demo/TRICORE_TC4x_QEMU_GCC $1=1 IGNORE_INTERRUPTS=$IGNORE_INTERRUPTS IGNORE_BYTES=$IGNORE_BYTES IGNORE_INTERNAL_STATE=$IGNORE_INTERNAL_STATE INSERT_WC=$INSERT_WC $EXTRA_MAKE_ARGS
    cp ../../../../OSEKOS/Demo/TRICORE_TC4x_QEMU_GCC/build/OSEKDemo.elf $BUILD_DIR/$(echo $1 | cut -d_ -f1 | tr '[:upper:]' '[:lower:]')$EXTRA_NAME_SUFFIX$2.elf
}

mkdir -p build

# Sequential inputs!
export PARTITION_INPUT=0
unset SPECIAL_CFLAGS

# Baseline
## Don't keep rng states
export IGNORE_INTERNAL_STATE=1
### Only bytes
export IGNORE_INTERRUPTS=1 IGNORE_BYTES=0 SUFFIX="_seq_bytes"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX
### Only interrupts
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=1 SUFFIX="_seq_int"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX
### Full
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_seq_full"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX
build POLYCOPTER_DEMO $SUFFIX

# Stateful -> presumably bad for us
## keep rng states
export IGNORE_INTERNAL_STATE=0
### Full
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_seq_stateful_full"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX

# Paritioned inputs
export PARTITION_INPUT=1

# Alternative input scheme
## Don't keep rng states
export IGNORE_INTERNAL_STATE=1
### Only bytes
export IGNORE_INTERRUPTS=1 IGNORE_BYTES=0 SUFFIX="_par_bytes"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX
### Only interrupts
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=1 SUFFIX="_par_int"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX
### Full
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_par_full"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX
build POLYCOPTER_DEMO $SUFFIX

# Stateful -> presumably bad for us
## keep rng states
export IGNORE_INTERNAL_STATE=0
### Full
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_par_stateful_full"
build WATERS_DEMO $SUFFIX
build RELEASE_DEMO $SUFFIX
build COPTER_DEMO $SUFFIX

# Stateful -> presumably bad for us
## keep rng states
export IGNORE_INTERNAL_STATE=0
export PARTITION_INPUT=0
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_seq_stateful_full"
build POLYCOPTER_DEMO $SUFFIX

# stateless + dataflow
export PARTITION_INPUT=0
export IGNORE_INTERNAL_STATE=1
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_seq_dataflow_full"
export SPECIAL_CFLAGS="-DCOPTER_DATAFLOW=1"
build POLYCOPTER_DEMO $SUFFIX
unset SPECIAL_CFLAGS

export PARTITION_INPUT=0
export IGNORE_INTERNAL_STATE=1
export IGNORE_INTERRUPTS=1 IGNORE_BYTES=0 SUFFIX="_seq_dataflow_bytes"
export SPECIAL_CFLAGS="-DCOPTER_DATAFLOW=1"
build POLYCOPTER_DEMO $SUFFIX
unset SPECIAL_CFLAGS

# stateless + dataflow
export PARTITION_INPUT=1
export IGNORE_INTERNAL_STATE=1
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_par_dataflow_full"
export SPECIAL_CFLAGS="-DCOPTER_DATAFLOW=1"
build POLYCOPTER_DEMO $SUFFIX
unset SPECIAL_CFLAGS


# special waters with no synchronization
export PARTITION_INPUT=0
export IGNORE_INTERNAL_STATE=1
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_seq_unsync_full"
export SPECIAL_CFLAGS="-DWATERS_UNSYNCHRONIZED=1"
build WATERS_DEMO $SUFFIX
unset SPECIAL_CFLAGS

# Create copies with special names
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc12_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc13_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc14_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc11_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc21_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc22_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc23_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc31_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc32_seq_full.elf
cp -f $BUILD_DIR/waters_seq_full.elf $BUILD_DIR/watersIc33_seq_full.elf

# =============================================================================
# OSEK/RTA_OS Demos for AURIX TC4x (TriCore)
# =============================================================================

# Build OSEK Blinky demo (baseline for OSEK testing)
export PARTITION_INPUT=0
export IGNORE_INTERNAL_STATE=1
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_seq_full"
build_osek BLINKY_DEMO $SUFFIX

# OSEK demo with only bytes input
export IGNORE_INTERRUPTS=1 IGNORE_BYTES=0 SUFFIX="_seq_bytes"
build_osek BLINKY_DEMO $SUFFIX

# OSEK demo with only interrupts
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=1 SUFFIX="_seq_int"
build_osek BLINKY_DEMO $SUFFIX

# OSEK partitioned inputs
export PARTITION_INPUT=1
export IGNORE_INTERRUPTS=0 IGNORE_BYTES=0 SUFFIX="_par_full"
build_osek BLINKY_DEMO $SUFFIX
