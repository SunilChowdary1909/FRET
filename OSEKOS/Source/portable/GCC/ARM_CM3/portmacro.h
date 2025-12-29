/*
 * OSEK/RTA_OS Portable Layer Macros for ARM Cortex-M3
 * GCC Toolchain
 *
 * This file provides architecture-specific type definitions and macros
 * for ARM Cortex-M3 processors.
 */

#ifndef PORTMACRO_H
#define PORTMACRO_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Type Definitions
 * ============================================================================ */

/* Architecture-specific types */
#define portCHAR            char
#define portFLOAT           float
#define portDOUBLE          double
#define portLONG            long
#define portSHORT           short
#define portSTACK_TYPE      uint32_t
#define portBASE_TYPE       long

typedef portSTACK_TYPE      StackType_t;
typedef long                BaseType_t;
typedef unsigned long       UBaseType_t;

/* Tick type for counters and alarms */
#if (configUSE_16_BIT_TICKS == 1)
    typedef uint16_t        TickType_t;
    #define portMAX_DELAY   (TickType_t)0xFFFF
#else
    typedef uint32_t        TickType_t;
    #define portMAX_DELAY   (TickType_t)0xFFFFFFFFUL
#endif

/* ============================================================================
 * Architecture Characteristics
 * ============================================================================ */

/* Stack grows downward on Cortex-M3 */
#define portSTACK_GROWTH    (-1)

/* Tick rate (default, can be overridden) */
#ifndef configTICK_RATE_HZ
    #define configTICK_RATE_HZ  1000
#endif

/* CPU clock (default, override for your board) */
#ifndef configCPU_CLOCK_HZ
    #define configCPU_CLOCK_HZ  72000000UL
#endif

/* Byte alignment for stack */
#define portBYTE_ALIGNMENT  8

/* Minimum stack size (in 32-bit words) */
#define portMINIMUM_STACK_SIZE  128

/* ============================================================================
 * Critical Section Macros
 * ============================================================================ */

extern void vPortEnterCritical(void);
extern void vPortExitCritical(void);
extern void portDISABLE_INTERRUPTS_impl(void);
extern void portENABLE_INTERRUPTS_impl(void);

#define portENTER_CRITICAL()    vPortEnterCritical()
#define portEXIT_CRITICAL()     vPortExitCritical()

#define portDISABLE_INTERRUPTS()    portDISABLE_INTERRUPTS_impl()
#define portENABLE_INTERRUPTS()     portENABLE_INTERRUPTS_impl()

/* ============================================================================
 * ISR-Safe Critical Section
 * ============================================================================ */

extern uint32_t portSET_INTERRUPT_MASK_FROM_ISR(void);
extern void portCLEAR_INTERRUPT_MASK_FROM_ISR(uint32_t);

#define portSET_INTERRUPT_MASK_FROM_ISR()       portSET_INTERRUPT_MASK_FROM_ISR()
#define portCLEAR_INTERRUPT_MASK_FROM_ISR(x)    portCLEAR_INTERRUPT_MASK_FROM_ISR(x)

/* ============================================================================
 * Yield Macros
 * ============================================================================ */

extern void vPortYield(void);
extern void vPortYieldFromISR(void);

#define portYIELD()             vPortYield()
#define portYIELD_FROM_ISR()    vPortYieldFromISR()

/* Force immediate yield via SVC */
#define portYIELD_WITHIN_API()  __asm volatile ("svc 0" ::: "memory")

/* ============================================================================
 * Task Function Type
 * ============================================================================ */

/* Task entry point type */
typedef void (*TaskFunction_t)(void *);

/* ============================================================================
 * Interrupt Handling
 * ============================================================================ */

/* Suspend all maskable interrupts */
#define portSUSPEND_ALL()                               \
    do {                                                \
        __asm volatile ("cpsid i" ::: "memory");        \
    } while(0)

/* Resume all maskable interrupts */
#define portRESUME_ALL()                                \
    do {                                                \
        __asm volatile ("cpsie i" ::: "memory");        \
    } while(0)

/* ============================================================================
 * Memory Barriers
 * ============================================================================ */

#define portMEMORY_BARRIER()    __asm volatile ("" ::: "memory")
#define portDATA_SYNC_BARRIER() __asm volatile ("dsb" ::: "memory")
#define portINSTR_SYNC_BARRIER() __asm volatile ("isb" ::: "memory")

/* ============================================================================
 * Bit Manipulation
 * ============================================================================ */

/* Count leading zeros (CLZ instruction) */
#define portGET_HIGHEST_PRIORITY(uxTopPriority, uxReadyPriorities)  \
    __asm volatile ("clz %0, %1" : "=r" (uxTopPriority) : "r" (uxReadyPriorities))

/* ============================================================================
 * Endianness
 * ============================================================================ */

/* Cortex-M3 is little-endian by default */
#define portBYTE_ORDER  pdLITTLE_ENDIAN

/* Reverse byte order in 32-bit word */
#define portREV32(x)    __builtin_bswap32(x)

/* Reverse byte order in 16-bit halfword */
#define portREV16(x)    __builtin_bswap16(x)

/* ============================================================================
 * Inline Assembly Helpers
 * ============================================================================ */

/* No operation */
#define portNOP()       __asm volatile ("nop")

/* Wait for interrupt (low power) */
#define portWFI()       __asm volatile ("wfi")

/* Wait for event */
#define portWFE()       __asm volatile ("wfe")

/* Send event */
#define portSEV()       __asm volatile ("sev")

/* ============================================================================
 * Kernel Priority Configuration
 * ============================================================================ */

/* Number of priority bits implemented in NVIC (Cortex-M3 has 3-8 bits) */
#ifndef configPRIO_BITS
    #define configPRIO_BITS     4
#endif

/* Maximum number of priority levels */
#define portMAX_PRIOGRUP    (1 << configPRIO_BITS)

/* Lowest interrupt priority (highest numerical value) */
#define portLOWEST_INTERRUPT_PRIORITY   ((1 << configPRIO_BITS) - 1)

/* ============================================================================
 * OSEK-Specific Macros
 * ============================================================================ */

/* Suspend OS interrupts (OSEK API) */
#define SuspendOSInterrupts()   portDISABLE_INTERRUPTS()

/* Resume OS interrupts (OSEK API) */
#define ResumeOSInterrupts()    portENABLE_INTERRUPTS()

/* Suspend all interrupts (OSEK API) */
#define SuspendAllInterrupts()  portDISABLE_INTERRUPTS()

/* Resume all interrupts (OSEK API) */
#define ResumeAllInterrupts()   portENABLE_INTERRUPTS()

/* ============================================================================
 * Stack Initialization
 * ============================================================================ */

extern StackType_t *pxPortInitialiseStack(StackType_t *pxTopOfStack,
                                           TaskFunction_t pxCode,
                                           void *pvParameters);

/* ============================================================================
 * Scheduler Control
 * ============================================================================ */

extern BaseType_t xPortStartScheduler(void);
extern void vPortEndScheduler(void);
extern void vPortSetupTimerInterrupt(uint32_t ulTickRateHz, uint32_t ulCPUClockHz);

/* ============================================================================
 * Task Control Block Access
 * ============================================================================ */

struct OSEK_TCB;  /* Forward declaration */
typedef struct OSEK_TCB OSEK_TCB_t;

extern void vPortSetCurrentTCB(OSEK_TCB_t *pxTCB);
extern OSEK_TCB_t *pxPortGetCurrentTCB(void);

/* ============================================================================
 * Weak Hook Prototypes
 * ============================================================================ */

extern void OSEK_ScheduleNextTask(void);
extern void OSEK_SVCHandler(uint32_t *pulStackFrame, uint8_t ucSVCNumber);
extern void OSEK_IncrementCounter(uint32_t CounterID);

#ifdef __cplusplus
}
#endif

#endif /* PORTMACRO_H */
