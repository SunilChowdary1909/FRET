# Makefile for OSEK/RTA_OS Demo on AURIX TC4x (TriCore)
# For use with QEMU TriCore emulation and FRET fuzzer

# Toolchain (HighTec TriCore or TASKING)
# Adjust paths as needed for your installation
CC = tricore-elf-gcc
AS = tricore-elf-as
LD = tricore-elf-ld
OBJCOPY = tricore-elf-objcopy
OBJDUMP = tricore-elf-objdump
SIZE = tricore-elf-size

# Output
BIN := OSEKDemo.elf
BUILD_DIR := build

# OSEK Kernel directories
OSEKOS_DIR_REL := ../../
OSEKOS_DIR := $(abspath $(OSEKOS_DIR_REL))
KERNEL_DIR := $(OSEKOS_DIR)/Source

# Source files
SOURCE_FILES := main.c
SOURCE_FILES += init/startup.c
SOURCE_FILES += init/crt0.S
SOURCE_FILES += syscall.c

# Kernel source files
SOURCE_FILES += $(KERNEL_DIR)/tasks.c
SOURCE_FILES += $(KERNEL_DIR)/resource.c
SOURCE_FILES += $(KERNEL_DIR)/alarm.c
SOURCE_FILES += $(KERNEL_DIR)/event.c
SOURCE_FILES += $(KERNEL_DIR)/portable/TriCore/AURIX_TC4x/port.c

# Include directories
INCLUDE_DIRS := -I.
INCLUDE_DIRS += -I$(KERNEL_DIR)/include
INCLUDE_DIRS += -I$(KERNEL_DIR)/portable/TriCore/AURIX_TC4x

# TriCore TC4x specific flags
# TC4x uses TC1.6.2 instruction set architecture
CPU_FLAGS := -mcpu=tc39xx
CPU_FLAGS += -mtc162

# Compiler flags
CFLAGS := $(CPU_FLAGS)
CFLAGS += -O2
CFLAGS += -g3
CFLAGS += -Wall -Wextra
CFLAGS += -ffunction-sections -fdata-sections
CFLAGS += $(INCLUDE_DIRS)

# Preprocessor definitions
CFLAGS += -DTRICORE
CFLAGS += -DAURIX_TC4x
CFLAGS += -DOS_CONFORMANCE_CLASS=OS_CONFORM_ECC1

# Assembler flags
ASFLAGS := $(CPU_FLAGS)
ASFLAGS += -g

# Linker flags
LDFLAGS := $(CPU_FLAGS)
LDFLAGS += -T linker.ld
LDFLAGS += -nostartfiles
LDFLAGS += -Wl,--gc-sections
LDFLAGS += -Wl,-Map=$(BUILD_DIR)/$(BIN:.elf=.map)

# Demo selection (similar to FreeRTOS demos)
ifeq ($(WATERS_DEMO), 1)
    SOURCE_FILES += main_waters.c
    CFLAGS += -DmainCREATE_WATERS_DEMO=1
else ifeq ($(COPTER_DEMO), 1)
    SOURCE_FILES += main_copter.c
    CFLAGS += -DmainCREATE_COPTER_DEMO=1
else ifeq ($(BLINKY_DEMO), 1)
    SOURCE_FILES += main_blinky.c
    CFLAGS += -DmainCREATE_BLINKY_DEMO=1
else
    # Default to blinky
    SOURCE_FILES += main_blinky.c
    CFLAGS += -DmainCREATE_BLINKY_DEMO=1
endif

# Fuzzer integration
ifeq ($(FUZZ_ENABLED), 1)
    SOURCE_FILES += fuzzhelper.c
    CFLAGS += -DFUZZ_ENABLED=1
endif

# Input handling flags (for fuzzer)
ifeq ($(IGNORE_INTERRUPTS), 1)
    CFLAGS += -DIGNORE_INTERRUPTS=1
endif
ifeq ($(IGNORE_BYTES), 1)
    CFLAGS += -DIGNORE_BYTES=1
endif
ifeq ($(IGNORE_INTERNAL_STATE), 1)
    CFLAGS += -DIGNORE_INTERNAL_STATE=1
endif
ifeq ($(INSERT_WC), 1)
    CFLAGS += -DINSERT_WC=1
endif

# Object files
OBJS := $(SOURCE_FILES:%.c=$(BUILD_DIR)/%.o)
OBJS := $(OBJS:%.S=$(BUILD_DIR)/%.o)

# Dependencies
DEPS := $(OBJS:.o=.d)

# Build rules
.PHONY: all clean size

all: $(BUILD_DIR)/$(BIN)

$(BUILD_DIR)/$(BIN): $(OBJS)
	@echo "Linking $@"
	$(CC) $(LDFLAGS) -o $@ $^
	@echo "Build complete"

$(BUILD_DIR)/%.o: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -MMD -MP -c -o $@ $<

$(BUILD_DIR)/%.o: %.S
	@mkdir -p $(dir $@)
	$(CC) $(ASFLAGS) -c -o $@ $<

$(BUILD_DIR)/%.o: $(KERNEL_DIR)/%.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -MMD -MP -c -o $@ $<

clean:
	rm -rf $(BUILD_DIR)

size: $(BUILD_DIR)/$(BIN)
	$(SIZE) $<

# Run in QEMU (use tc4x CPU type for AURIX TC4x)
run: $(BUILD_DIR)/$(BIN)
	qemu-system-tricore -M tricore_testboard -cpu tc4x -nographic -semihosting -kernel $<

# Debug in QEMU
debug: $(BUILD_DIR)/$(BIN)
	qemu-system-tricore -M tricore_testboard -cpu tc4x -nographic -semihosting -kernel $< -S -s

# Include dependencies
-include $(DEPS)
