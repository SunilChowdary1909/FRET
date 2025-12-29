#!/usr/bin/env python3
import sys

if len(sys.argv) < 2:
    print("Usage: time.py <number>")
    sys.exit(1)

try:
    number = float(sys.argv[1])
except ValueError:
    print("The first argument must be a number.")
    sys.exit(1)

QEMU_SHIFT=5
ISNS_PER_US=10**3 / (2**QEMU_SHIFT)
int_offset=53430

if len(sys.argv) == 2:
    print("Time span")
    print("ISNS -> µs", f"{number / ISNS_PER_US:.2f} us")
    print("µs -> ISNS", f"{number * ISNS_PER_US:.2f}")
    print("Interrupt offset")
    print("ISNS -> µs", f"{((number + int_offset) / ISNS_PER_US):.2f} us")
    print("µs -> ISNS", f"{((number * ISNS_PER_US)-int_offset):.2f}")
elif len(sys.argv) > 2:
    for i in range(1, len(sys.argv)):
        try:
            number = float(sys.argv[i])
        except ValueError:
            print(f"The argument {i} must be a number.")
            sys.exit(1)
        print(f"{((number + int_offset) / (ISNS_PER_US*1000)):.2f}")
