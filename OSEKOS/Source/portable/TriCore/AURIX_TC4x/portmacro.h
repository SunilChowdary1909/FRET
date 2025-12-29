/*
 * Port Macros for AURIX TC4x (TriCore Architecture)
 */

#ifndef PORTMACRO_H
#define PORTMACRO_H

#include <stdint.h>

/*============================================================================
 * Architecture-Specific Types
 *============================================================================*/

#define portCHAR        char
#define portFLOAT       float
#define portDOUBLE      double
#define portLONG        long
#define portSHORT       short
#define portSTACK_TYPE  uint32_t
#define portBASE_TYPE   int32_t

typedef portSTACK_TYPE  StackType_t;
typedef int32_t         BaseType_t;
typedef uint32_t        UBaseType_t;
typedef uint32_t        TickType_t;

#define portMAX_DELAY   ((TickType_t)0xFFFFFFFFU)

/*============================================================================
 * TriCore/TC4x Architecture Constants
 *============================================================================*/

/* Stack grows from high to low on TriCore */
#define portSTACK_GROWTH            (-1)

/* Byte alignment for stack */
#define portBYTE_ALIGNMENT          8

/* Minimum stack size (in 32-bit words) */
#define portMINIMAL_STACK_SIZE      256

/* Tick rate in Hz */
#define configTICK_RATE_HZ          1000

/* Maximum priority levels */
#define configMAX_PRIORITIES        32

/*============================================================================
 * Context Save Area (CSA) Configuration
 *============================================================================*/

/* CSA frame size in bytes (16 words * 4 bytes) */
#define portCSA_FRAME_SIZE          64

/* Number of CSA frames to allocate per task (upper + lower) */
#define portCSA_FRAMES_PER_TASK     2

/* Syscall number for context switch */
#define OS_SYSCALL_CONTEXT_SWITCH   0

/*============================================================================
 * Critical Section Macros
 *============================================================================*/

#define portENTER_CRITICAL()        Os_EnterCritical()
#define portEXIT_CRITICAL()         Os_ExitCritical()

#define portDISABLE_INTERRUPTS()    DisableAllInterrupts()
#define portENABLE_INTERRUPTS()     EnableAllInterrupts()

/*============================================================================
 * Task Utilities
 *============================================================================*/

/* Yield to scheduler (request context switch) */
#define portYIELD()                 Os_RequestContextSwitch()

/* Yield from ISR */
#define portYIELD_FROM_ISR(x)       do { if(x) Os_Ocb.contextSwitchNeeded = true; } while(0)

/* End of ISR - check for pending context switch */
#define portEND_SWITCHING_ISR(x)    portYIELD_FROM_ISR(x)

/*============================================================================
 * Memory Barrier Macros
 *============================================================================*/

#define portMEMORY_BARRIER()        __asm__ volatile ("dsync" ::: "memory")

/*============================================================================
 * Trap/Interrupt Vector Numbers (TC4x specific)
 *============================================================================*/

/* Trap classes */
#define TRAP_CLASS_MMU              0
#define TRAP_CLASS_PROTECTION       1
#define TRAP_CLASS_INSTRUCTION      2
#define TRAP_CLASS_CONTEXT          3
#define TRAP_CLASS_BUS              4
#define TRAP_CLASS_ASSERTION        5
#define TRAP_CLASS_SYSCALL          6
#define TRAP_CLASS_NMI              7

/* Common interrupt priorities */
#define OS_TICK_INTERRUPT_PRIORITY  1
#define OS_PENDSV_PRIORITY          255  /* Lowest priority for context switch */

/*============================================================================
 * Compiler-Specific Attributes
 *============================================================================*/

#ifdef __GNUC__
    #define portINLINE              inline __attribute__((always_inline))
    #define portNOINLINE            __attribute__((noinline))
    #define portNAKED               __attribute__((naked))
    #define portWEAK                __attribute__((weak))
    #define portALIGN(x)            __attribute__((aligned(x)))
    #define portPACKED              __attribute__((packed))
    #define portUSED                __attribute__((used))
    #define portUNUSED              __attribute__((unused))
    #define portISR(name)           void __attribute__((interrupt_handler)) name(void)
#else
    #define portINLINE              inline
    #define portNOINLINE
    #define portNAKED
    #define portWEAK
    #define portALIGN(x)
    #define portPACKED
    #define portUSED
    #define portUNUSED
    #define portISR(name)           void name(void)
#endif

/*============================================================================
 * External Function Declarations
 *============================================================================*/

extern void Os_EnterCritical(void);
extern void Os_ExitCritical(void);
extern void Os_RequestContextSwitch(void);
extern void Os_ContextSwitchHandler(void);
extern void Os_AddToReadyQueue(void* task);
extern void Os_RemoveFromReadyQueue(void* task);
extern void* Os_GetHighestPriorityTask(void);

/* Global OS control block (forward declaration) */
struct Os_ControlBlock;
extern struct Os_ControlBlock Os_Ocb;

#endif /* PORTMACRO_H */
