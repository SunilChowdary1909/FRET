/*
 * TriCore TC4x Startup Code
 * Assembly startup for OSEK Demo on QEMU
 */

    .section .startup, "ax"
    .global _start
    .type _start, @function

_start:
    /* Disable watchdog timers (for QEMU this is a no-op) */
    
    /* Initialize stack pointer (A[10]) */
    movh.a  %a10, hi:__STACK_START
    lea     %a10, [%a10] lo:__STACK_START
    
    /* Initialize global pointer (A[0] = small data area) */
    movh.a  %a0, hi:_SMALL_DATA_
    lea     %a0, [%a0] lo:_SMALL_DATA_
    
    /* Initialize global pointer (A[1] = small data area 2) */
    movh.a  %a1, hi:_SMALL_DATA2_
    lea     %a1, [%a1] lo:_SMALL_DATA2_
    
    /* Initialize global pointer (A[8] = small data area 3) */
    movh.a  %a8, hi:_SMALL_DATA3_
    lea     %a8, [%a8] lo:_SMALL_DATA3_
    
    /* Initialize global pointer (A[9] = small data area 4) */
    movh.a  %a9, hi:_SMALL_DATA4_
    lea     %a9, [%a9] lo:_SMALL_DATA4_
    
    /* Initialize CSA (Context Save Area) */
    call    _init_csa
    
    /* Clear BSS section */
    call    _clear_bss
    
    /* Copy initialized data from flash to RAM */
    call    _copy_data
    
    /* Call C++ constructors */
    call    _call_ctors
    
    /* Jump to main */
    call    main
    
    /* If main returns, loop forever */
_exit:
    wait
    j       _exit

    .size _start, . - _start

/*
 * Initialize Context Save Area (CSA)
 * Sets up the free context list for TriCore's automatic context switching
 */
    .global _init_csa
    .type _init_csa, @function
_init_csa:
    /* Get CSA start and end addresses */
    movh.a  %a15, hi:__CSA_START
    lea     %a15, [%a15] lo:__CSA_START
    movh.a  %a14, hi:__CSA_END
    lea     %a14, [%a14] lo:__CSA_END
    
    /* Calculate number of CSA frames (64 bytes each) */
    /* Each frame's first word points to next frame */
    
    /* Convert addresses to CSA link format */
    /* Link format: [19:16] = segment, [15:0] = offset >> 6 */
    mov.a   %a13, %a15              /* Current frame pointer */
    
_csa_loop:
    /* Check if we've reached the end */
    jge.a   %a13, %a14, _csa_done
    
    /* Calculate next frame address */
    lea     %a12, [%a13] 64
    
    /* Check if next frame is beyond end */
    jge.a   %a12, %a14, _csa_last
    
    /* Create link to next frame */
    /* Extract segment (bits 31:28 -> bits 19:16) */
    mov.d   %d15, %a12
    extr.u  %d14, %d15, 28, 4
    sh      %d14, %d14, 16
    
    /* Extract offset >> 6 (bits 21:6 -> bits 15:0) */
    extr.u  %d13, %d15, 6, 16
    
    /* Combine segment and offset */
    or      %d12, %d14, %d13
    
    /* Store link in current frame */
    st.w    [%a13], %d12
    
    /* Move to next frame */
    mov.a   %a13, %a12
    j       _csa_loop
    
_csa_last:
    /* Last frame - store 0 as link (end of list) */
    mov     %d12, 0
    st.w    [%a13], %d12
    
_csa_done:
    /* Set FCX (Free Context List) to first CSA frame */
    mov.d   %d15, %a15
    extr.u  %d14, %d15, 28, 4
    sh      %d14, %d14, 16
    extr.u  %d13, %d15, 6, 16
    or      %d12, %d14, %d13
    mtcr    $fcx, %d12
    isync
    
    /* Set LCX (Last Context) to last CSA frame for overflow detection */
    sub.a   %a13, %a14, 64
    mov.d   %d15, %a13
    extr.u  %d14, %d15, 28, 4
    sh      %d14, %d14, 16
    extr.u  %d13, %d15, 6, 16
    or      %d12, %d14, %d13
    mtcr    $lcx, %d12
    isync
    
    ret
    
    .size _init_csa, . - _init_csa

/*
 * Clear BSS section
 */
    .global _clear_bss
    .type _clear_bss, @function
_clear_bss:
    movh.a  %a15, hi:__BSS_START
    lea     %a15, [%a15] lo:__BSS_START
    movh.a  %a14, hi:__BSS_END
    lea     %a14, [%a14] lo:__BSS_END
    
    mov     %d15, 0
    
_bss_loop:
    jge.a   %a15, %a14, _bss_done
    st.w    [%a15+], %d15
    j       _bss_loop
    
_bss_done:
    ret
    
    .size _clear_bss, . - _clear_bss

/*
 * Copy initialized data from flash to RAM
 */
    .global _copy_data
    .type _copy_data, @function
_copy_data:
    movh.a  %a15, hi:__DATA_LOAD
    lea     %a15, [%a15] lo:__DATA_LOAD
    movh.a  %a14, hi:__DATA_START
    lea     %a14, [%a14] lo:__DATA_START
    movh.a  %a13, hi:__DATA_END
    lea     %a13, [%a13] lo:__DATA_END
    
_data_loop:
    jge.a   %a14, %a13, _data_done
    ld.w    %d15, [%a15+]
    st.w    [%a14+], %d15
    j       _data_loop
    
_data_done:
    ret
    
    .size _copy_data, . - _copy_data

/*
 * Call C++ constructors
 */
    .global _call_ctors
    .type _call_ctors, @function
_call_ctors:
    movh.a  %a15, hi:__CTOR_LIST__
    lea     %a15, [%a15] lo:__CTOR_LIST__
    movh.a  %a14, hi:__CTOR_END__
    lea     %a14, [%a14] lo:__CTOR_END__
    
_ctor_loop:
    jge.a   %a15, %a14, _ctor_done
    ld.a    %a12, [%a15+]
    calli   %a12
    j       _ctor_loop
    
_ctor_done:
    ret
    
    .size _call_ctors, . - _call_ctors

/*
 * Trap vector table
 */
    .section .traptab, "ax"
    .align 256
    .global _trap_table
_trap_table:
    /* Class 0: MMU */
    .org 0x00
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_MMU
    lea     %a14, [%a14] lo:Os_TrapHandler_MMU
    ji      %a14
    
    /* Class 1: Protection */
    .org 0x20
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_Protection
    lea     %a14, [%a14] lo:Os_TrapHandler_Protection
    ji      %a14
    
    /* Class 2: Instruction */
    .org 0x40
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_Instruction
    lea     %a14, [%a14] lo:Os_TrapHandler_Instruction
    ji      %a14
    
    /* Class 3: Context */
    .org 0x60
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_Context
    lea     %a14, [%a14] lo:Os_TrapHandler_Context
    ji      %a14
    
    /* Class 4: Bus */
    .org 0x80
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_Bus
    lea     %a14, [%a14] lo:Os_TrapHandler_Bus
    ji      %a14
    
    /* Class 5: Assertion */
    .org 0xA0
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_Assertion
    lea     %a14, [%a14] lo:Os_TrapHandler_Assertion
    ji      %a14
    
    /* Class 6: Syscall (used for context switch) */
    .org 0xC0
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_Syscall
    lea     %a14, [%a14] lo:Os_TrapHandler_Syscall
    ji      %a14
    
    /* Class 7: NMI */
    .org 0xE0
    svlcx
    movh.a  %a14, hi:Os_TrapHandler_NMI
    lea     %a14, [%a14] lo:Os_TrapHandler_NMI
    ji      %a14

/*
 * Small data area symbols (required by compiler)
 */
    .section .sdata, "aw"
    .global _SMALL_DATA_
_SMALL_DATA_:
    .global _SMALL_DATA2_
_SMALL_DATA2_:
    .global _SMALL_DATA3_
_SMALL_DATA3_:
    .global _SMALL_DATA4_
_SMALL_DATA4_:
