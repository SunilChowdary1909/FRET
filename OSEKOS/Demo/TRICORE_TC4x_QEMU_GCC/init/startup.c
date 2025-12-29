/*
 * TriCore TC4x Startup C Code
 * Hardware initialization and trap handlers
 */

#include "../osek.h"
#include "../osek_types.h"

/*============================================================================
 * Trap Handlers
 *============================================================================*/

/* Default trap handler - loops forever */
static void trap_default(void) {
    while (1) {
        __asm__ volatile ("wait");
    }
}

/* Trap Class 0: MMU */
void Os_TrapHandler_MMU(void) {
    trap_default();
}

/* Trap Class 1: Protection Error */
void Os_TrapHandler_Protection(void) {
    trap_default();
}

/* Trap Class 2: Instruction Error */
void Os_TrapHandler_Instruction(void) {
    trap_default();
}

/* Trap Class 3: Context Error (CSA overflow/underflow) */
void Os_TrapHandler_Context(void) {
    /* This could indicate stack overflow */
    trap_default();
}

/* Trap Class 4: Bus Error */
void Os_TrapHandler_Bus(void) {
    trap_default();
}

/* Trap Class 5: Assertion */
void Os_TrapHandler_Assertion(void) {
    trap_default();
}

/* Trap Class 6: Syscall - Used for context switch */
void Os_TrapHandler_Syscall(void) {
    /* Get TIN (Trap Identification Number) from D[15] */
    uint32_t tin;
    __asm__ volatile ("mov %0, %%d15" : "=d"(tin));
    
    if (tin == 0) {
        /* Context switch request */
        Os_ContextSwitchHandler();
    }
    
    /* Return from trap */
    __asm__ volatile ("rfe");
}

/* Trap Class 7: NMI */
void Os_TrapHandler_NMI(void) {
    trap_default();
}

/*============================================================================
 * Early Hardware Initialization (called from startup)
 *============================================================================*/

void startup_init(void) {
    /* 
     * On real hardware, we would:
     * - Disable watchdogs
     * - Configure clocks
     * - Enable caches
     * - Configure memory protection
     * 
     * For QEMU, most of this is not needed.
     */
}
