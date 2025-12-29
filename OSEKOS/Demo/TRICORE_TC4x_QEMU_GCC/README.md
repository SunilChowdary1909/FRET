# OSEK/RTA_OS Demo for AURIX TC4x

## Overview

This is an OSEK-compliant real-time operating system demo for the Infineon AURIX TC4x (TriCore) architecture, designed to run on QEMU and be fuzzed by the FRET fuzzer.

## Structure

```
TRICORE_TC4x_QEMU_GCC/
├── Makefile              # Build system
├── linker.ld             # TriCore linker script
├── main.c                # Main entry point
├── main_blinky.c         # Simple blinky demo
├── main_waters.c         # Waters benchmark (TODO)
├── main_copter.c         # Copter demo (TODO)
├── init/
│   ├── crt0.S            # Startup assembly (CSA init, vectors)
│   └── startup.c         # Trap handlers, HW init
└── README.md             # This file
```

## Requirements

- **TriCore Toolchain**: HighTec TriCore GCC or TASKING VX toolchain
  - Set `CC=tricore-elf-gcc` in Makefile
- **QEMU with TriCore support**: The qemu-libafl-bridge has TriCore support (tc27x, tc37x)
  - TC4x uses TC1.6.2 ISA which is compatible with tc37x

## Building

```bash
# Build default (blinky) demo
make

# Build Waters benchmark
make WATERS_DEMO=1

# Build with fuzzer integration
make FUZZ_ENABLED=1

# Clean
make clean
```

## Running in QEMU

```bash
# Run (uses tc4x CPU type added to QEMU for TC4x support)
make run

# Debug (starts GDB server on port 1234)
make debug
```

## OSEK Conformance

This demo implements OSEK/VDX OS 2.2.3 specification:
- **Conformance Class**: ECC1 (Extended, single activation)
- **Scheduling Policy**: Full preemptive
- **Features**:
  - Task management (ActivateTask, TerminateTask, ChainTask, Schedule)
  - Resource management (GetResource, ReleaseResource) with priority ceiling
  - Event control (SetEvent, WaitEvent, ClearEvent, GetEvent)
  - Alarms and counters (SetRelAlarm, SetAbsAlarm, CancelAlarm)
  - Category 2 ISRs

## TriCore Architecture Notes

- **Context Save Area (CSA)**: TriCore uses hardware-managed context switching via linked CSA frames
- **Traps**: 8 trap classes (0-7), with Class 6 (Syscall) used for OS context switch
- **Stack**: Grows downward, 8-byte aligned
- **Registers**: 16 data registers (D0-D15), 16 address registers (A0-A15)

## Integration with FRET Fuzzer

The `FUZZ_INPUT` buffer is placed in a special section and can be accessed by the fuzzer. The `trigger_Qemu_break` function signals execution completion.

Build with fuzzing support:
```bash
make FUZZ_ENABLED=1 IGNORE_INTERRUPTS=0 IGNORE_BYTES=0
```

## TODO

- [ ] Add TC4x CPU model to QEMU (currently uses tc37x)
- [ ] Implement Waters and Copter demos
- [ ] Add more OSEK conformance tests
- [ ] Implement full interrupt handling
