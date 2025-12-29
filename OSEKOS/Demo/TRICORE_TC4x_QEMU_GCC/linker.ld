/*
 * TriCore TC4x Linker Script for QEMU
 * Memory layout for OSEK Demo
 */

/* Memory regions for TC4x (simplified for QEMU) */
MEMORY
{
    /* Program Flash (PFLASH) - 16MB typical for TC4x */
    PFLASH (rx)  : ORIGIN = 0x80000000, LENGTH = 4M
    
    /* Data Flash (DFLASH) */
    DFLASH (r)   : ORIGIN = 0xAF000000, LENGTH = 1M
    
    /* Program SRAM (PSRAM) */
    PSRAM (rwx)  : ORIGIN = 0x70000000, LENGTH = 64K
    
    /* Data SRAM (DSRAM) - CPU0 local */
    DSRAM0 (rw)  : ORIGIN = 0x70100000, LENGTH = 240K
    
    /* LMU RAM (shared) */
    LMU (rw)     : ORIGIN = 0x90000000, LENGTH = 32K
    
    /* CSA Region (Context Save Area) */
    CSA (rw)     : ORIGIN = 0x70140000, LENGTH = 16K
}

/* Entry point */
ENTRY(_start)

/* Stack and heap sizes */
__STACK_SIZE = 8K;
__HEAP_SIZE = 4K;
__CSA_SIZE = 16K;

SECTIONS
{
    /* Startup code and interrupt vectors */
    .startup : ALIGN(4)
    {
        KEEP(*(.startup))
        KEEP(*(.traptab))
        KEEP(*(.inttab))
    } > PFLASH

    /* Program code */
    .text : ALIGN(4)
    {
        *(.text)
        *(.text.*)
        *(.gnu.linkonce.t.*)
    } > PFLASH

    /* Read-only data */
    .rodata : ALIGN(4)
    {
        *(.rodata)
        *(.rodata.*)
        *(.gnu.linkonce.r.*)
    } > PFLASH

    /* C++ constructors */
    .ctors : ALIGN(4)
    {
        __CTOR_LIST__ = .;
        KEEP(*(.ctors))
        KEEP(*(SORT(.ctors.*)))
        __CTOR_END__ = .;
    } > PFLASH

    /* C++ destructors */
    .dtors : ALIGN(4)
    {
        __DTOR_LIST__ = .;
        KEEP(*(.dtors))
        KEEP(*(SORT(.dtors.*)))
        __DTOR_END__ = .;
    } > PFLASH

    /* Initialized data - copied from flash to RAM */
    .data : ALIGN(4)
    {
        __DATA_START = .;
        *(.data)
        *(.data.*)
        *(.gnu.linkonce.d.*)
        __DATA_END = .;
    } > DSRAM0 AT > PFLASH

    __DATA_LOAD = LOADADDR(.data);
    __DATA_SIZE = SIZEOF(.data);

    /* Fuzz input buffer - special section for fuzzer */
    .fuzz_input (NOLOAD) : ALIGN(4)
    {
        FUZZ_INPUT = .;
        *(.fuzz_input)
        . = . + 4096;
    } > DSRAM0

    /* Uninitialized data (BSS) */
    .bss (NOLOAD) : ALIGN(4)
    {
        __BSS_START = .;
        *(.bss)
        *(.bss.*)
        *(.gnu.linkonce.b.*)
        *(COMMON)
        __BSS_END = .;
    } > DSRAM0

    __BSS_SIZE = SIZEOF(.bss);

    /* Heap */
    .heap (NOLOAD) : ALIGN(8)
    {
        __HEAP_START = .;
        . = . + __HEAP_SIZE;
        __HEAP_END = .;
    } > DSRAM0

    /* Stack */
    .stack (NOLOAD) : ALIGN(8)
    {
        __STACK_END = .;
        . = . + __STACK_SIZE;
        __STACK_START = .;
    } > DSRAM0

    /* Context Save Area (CSA) for TriCore */
    .csa (NOLOAD) : ALIGN(64)
    {
        __CSA_START = .;
        . = . + __CSA_SIZE;
        __CSA_END = .;
    } > CSA

    /* Debug info */
    .debug_info     0 : { *(.debug_info) }
    .debug_abbrev   0 : { *(.debug_abbrev) }
    .debug_line     0 : { *(.debug_line) }
    .debug_frame    0 : { *(.debug_frame) }
    .debug_str      0 : { *(.debug_str) }
    .debug_loc      0 : { *(.debug_loc) }
    .debug_ranges   0 : { *(.debug_ranges) }
    
    /* Discard */
    /DISCARD/ :
    {
        *(.comment)
        *(.note.*)
    }
}

/* Symbols for startup code */
PROVIDE(__TRICORE_DERIVATE_MEMORY_MAP__ = 0);
PROVIDE(__TRICORE_CORE_TC162__ = 1);
