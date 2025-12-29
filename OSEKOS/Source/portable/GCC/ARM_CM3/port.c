/*
 * OSEK/RTA_OS Portable Layer for ARM Cortex-M3
 * GCC Toolchain
 *
 * This file provides the architecture-specific implementation for
 * ARM Cortex-M3 processors (e.g., STM32F1, LPC17xx, etc.)
 */

#include "osek.h"
#include "portmacro.h"

/* ============================================================================
 * Cortex-M3 System Registers
 * ============================================================================ */

/* System Control Block registers */
#define SCB_ICSR        (*(volatile uint32_t *)0xE000ED04)  /* Interrupt Control State */
#define SCB_VTOR        (*(volatile uint32_t *)0xE000ED08)  /* Vector Table Offset */
#define SCB_AIRCR       (*(volatile uint32_t *)0xE000ED0C)  /* Application Interrupt Reset Control */
#define SCB_SCR         (*(volatile uint32_t *)0xE000ED10)  /* System Control Register */
#define SCB_SHPR2       (*(volatile uint32_t *)0xE000ED1C)  /* System Handler Priority 2 (SVCall) */
#define SCB_SHPR3       (*(volatile uint32_t *)0xE000ED20)  /* System Handler Priority 3 (PendSV, SysTick) */

/* ICSR bits */
#define ICSR_PENDSVSET  (1UL << 28)
#define ICSR_PENDSVCLR  (1UL << 27)

/* NVIC registers */
#define NVIC_ISER0      (*(volatile uint32_t *)0xE000E100)
#define NVIC_ICER0      (*(volatile uint32_t *)0xE000E180)
#define NVIC_IPR0       ((volatile uint8_t *)0xE000E400)

/* SysTick registers */
#define SYSTICK_CSR     (*(volatile uint32_t *)0xE000E010)  /* Control and Status */
#define SYSTICK_RVR     (*(volatile uint32_t *)0xE000E014)  /* Reload Value */
#define SYSTICK_CVR     (*(volatile uint32_t *)0xE000E018)  /* Current Value */

/* ============================================================================
 * Port Configuration
 * ============================================================================ */

/* Stack grows downward on ARM */
#define portSTACK_GROWTH    (-1)

/* Minimum stack size in words (32-bit) */
#define portMINIMUM_STACK_SIZE  128

/* Byte alignment for stack */
#define portBYTE_ALIGNMENT  8

/* Initial PSR value for new tasks (Thumb mode) */
#define portINITIAL_XPSR    0x01000000UL

/* ============================================================================
 * Private Variables
 * ============================================================================ */

/* Currently running task TCB pointer */
static OSEK_TCB_t *pxCurrentTCB = NULL;

/* Critical nesting counter */
static volatile uint32_t ulCriticalNesting = 0;

/* Scheduler running flag */
static volatile uint32_t ulSchedulerRunning = 0;

/* ============================================================================
 * Exception Priorities
 * ============================================================================ */

/* Lowest priority for PendSV (used for context switch) */
#define configKERNEL_INTERRUPT_PRIORITY     0xFF

/* SVCall priority (OS system calls) */
#define configSVCALL_INTERRUPT_PRIORITY     0x00

/* ============================================================================
 * Stack Initialization
 * ============================================================================ */

/*
 * Initialize the stack for a new task
 * Sets up the initial stack frame as if the task had been interrupted
 *
 * Stack frame layout (from high to low address):
 * - xPSR (Program Status Register)
 * - PC (Return Address - task entry point)
 * - LR (Link Register)
 * - R12
 * - R3
 * - R2
 * - R1
 * - R0 (parameter to task)
 * - R11
 * - R10
 * - R9
 * - R8
 * - R7
 * - R6
 * - R5
 * - R4
 */
StackType_t *pxPortInitialiseStack(StackType_t *pxTopOfStack,
                                    TaskFunction_t pxCode,
                                    void *pvParameters)
{
    /* Simulate the stack frame as if a context switch has occurred */
    
    /* Align stack to 8 bytes (AAPCS requirement) */
    pxTopOfStack = (StackType_t *)((uint32_t)pxTopOfStack & ~0x7UL);
    
    /* Offset for full descending stack */
    pxTopOfStack--;
    
    /* xPSR - Thumb bit set */
    *pxTopOfStack = portINITIAL_XPSR;
    pxTopOfStack--;
    
    /* PC - Entry point of the task */
    *pxTopOfStack = ((uint32_t)pxCode) & ~0x1UL;  /* Ensure bit 0 is clear */
    pxTopOfStack--;
    
    /* LR - Return from task (should never happen for OSEK tasks) */
    *pxTopOfStack = 0xFFFFFFFDUL;  /* EXC_RETURN: return to Thread mode, PSP */
    pxTopOfStack--;
    
    /* R12 */
    *pxTopOfStack = 0x12121212UL;
    pxTopOfStack--;
    
    /* R3 */
    *pxTopOfStack = 0x03030303UL;
    pxTopOfStack--;
    
    /* R2 */
    *pxTopOfStack = 0x02020202UL;
    pxTopOfStack--;
    
    /* R1 */
    *pxTopOfStack = 0x01010101UL;
    pxTopOfStack--;
    
    /* R0 - Task parameter */
    *pxTopOfStack = (uint32_t)pvParameters;
    pxTopOfStack--;
    
    /* Remaining registers saved manually during context switch */
    /* R11 */
    *pxTopOfStack = 0x11111111UL;
    pxTopOfStack--;
    
    /* R10 */
    *pxTopOfStack = 0x10101010UL;
    pxTopOfStack--;
    
    /* R9 */
    *pxTopOfStack = 0x09090909UL;
    pxTopOfStack--;
    
    /* R8 */
    *pxTopOfStack = 0x08080808UL;
    pxTopOfStack--;
    
    /* R7 */
    *pxTopOfStack = 0x07070707UL;
    pxTopOfStack--;
    
    /* R6 */
    *pxTopOfStack = 0x06060606UL;
    pxTopOfStack--;
    
    /* R5 */
    *pxTopOfStack = 0x05050505UL;
    pxTopOfStack--;
    
    /* R4 */
    *pxTopOfStack = 0x04040404UL;
    
    return pxTopOfStack;
}

/* ============================================================================
 * Critical Section Management
 * ============================================================================ */

void vPortEnterCritical(void)
{
    portDISABLE_INTERRUPTS();
    ulCriticalNesting++;
    __asm volatile ("dsb" ::: "memory");
    __asm volatile ("isb");
}

void vPortExitCritical(void)
{
    if (ulCriticalNesting > 0)
    {
        ulCriticalNesting--;
        
        if (ulCriticalNesting == 0)
        {
            portENABLE_INTERRUPTS();
        }
    }
}

/* ============================================================================
 * Interrupt Control
 * ============================================================================ */

void portDISABLE_INTERRUPTS_impl(void)
{
    __asm volatile (
        "cpsid i    \n"
        "dsb        \n"
        "isb        \n"
        ::: "memory"
    );
}

void portENABLE_INTERRUPTS_impl(void)
{
    __asm volatile (
        "cpsie i    \n"
        ::: "memory"
    );
}

uint32_t portSET_INTERRUPT_MASK_FROM_ISR(void)
{
    uint32_t ulReturn;
    
    __asm volatile (
        "mrs %0, basepri    \n"
        "mov r1, %1         \n"
        "msr basepri, r1    \n"
        : "=r" (ulReturn)
        : "i" (configKERNEL_INTERRUPT_PRIORITY)
        : "r1", "memory"
    );
    
    return ulReturn;
}

void portCLEAR_INTERRUPT_MASK_FROM_ISR(uint32_t ulNewMask)
{
    __asm volatile (
        "msr basepri, %0    \n"
        :
        : "r" (ulNewMask)
        : "memory"
    );
}

/* ============================================================================
 * Context Switch
 * ============================================================================ */

/*
 * Trigger a context switch by setting PendSV pending
 * PendSV has lowest priority so it runs after all ISRs complete
 */
void vPortYield(void)
{
    /* Set PendSV pending */
    SCB_ICSR = ICSR_PENDSVSET;
    
    /* Barriers to ensure the write takes effect immediately */
    __asm volatile ("dsb" ::: "memory");
    __asm volatile ("isb");
}

/*
 * Yield from ISR context
 */
void vPortYieldFromISR(void)
{
    SCB_ICSR = ICSR_PENDSVSET;
}

/* ============================================================================
 * PendSV Handler - Context Switch (Naked Function)
 * ============================================================================ */

__attribute__((naked)) void PendSV_Handler(void)
{
    __asm volatile (
        /* Disable interrupts */
        "cpsid i                        \n"
        
        /* Get current PSP */
        "mrs r0, psp                    \n"
        
        /* Check if this is first context switch (pxCurrentTCB == NULL) */
        "ldr r3, =pxCurrentTCB          \n"
        "ldr r2, [r3]                   \n"
        "cbz r2, skip_save              \n"
        
        /* Save remaining registers (R4-R11) */
        "stmdb r0!, {r4-r11}            \n"
        
        /* Save new stack pointer to current TCB */
        "str r0, [r2]                   \n"  /* TCB->pxTopOfStack = SP */
        
        "skip_save:                     \n"
        
        /* Call scheduler to select next task */
        "push {r3, lr}                  \n"
        "bl OSEK_ScheduleNextTask       \n"
        "pop {r3, lr}                   \n"
        
        /* Get new current TCB */
        "ldr r2, [r3]                   \n"
        
        /* Get new task's stack pointer */
        "ldr r0, [r2]                   \n"
        
        /* Restore registers R4-R11 */
        "ldmia r0!, {r4-r11}            \n"
        
        /* Set PSP to new stack */
        "msr psp, r0                    \n"
        
        /* Enable interrupts */
        "cpsie i                        \n"
        
        /* Return from exception (will restore R0-R3, R12, LR, PC, xPSR) */
        "bx lr                          \n"
        
        /* Align to word boundary */
        ".align 4                       \n"
    );
}

/* ============================================================================
 * SVC Handler - System Calls (Naked Function)
 * ============================================================================ */

__attribute__((naked)) void SVC_Handler(void)
{
    __asm volatile (
        /* Determine which stack was used (MSP or PSP) */
        "tst lr, #4                     \n"
        "ite eq                         \n"
        "mrseq r0, msp                  \n"
        "mrsne r0, psp                  \n"
        
        /* r0 now points to stacked frame */
        /* Get SVC number from stacked PC */
        "ldr r1, [r0, #24]              \n"  /* r1 = stacked PC */
        "ldrb r1, [r1, #-2]             \n"  /* r1 = SVC number */
        
        /* Branch to C handler */
        "push {lr}                      \n"
        "bl OSEK_SVCHandler             \n"
        "pop {pc}                       \n"
    );
}

/* ============================================================================
 * SysTick Handler - System Tick
 * ============================================================================ */

void SysTick_Handler(void)
{
    uint32_t ulSavedInterruptStatus;
    
    ulSavedInterruptStatus = portSET_INTERRUPT_MASK_FROM_ISR();
    
    /* Increment system tick counter */
    OSEK_IncrementCounter(0);  /* Counter 0 is system counter */
    
    /* Check if context switch is needed */
    if (ulSchedulerRunning)
    {
        vPortYieldFromISR();
    }
    
    portCLEAR_INTERRUPT_MASK_FROM_ISR(ulSavedInterruptStatus);
}

/* ============================================================================
 * Port Initialization
 * ============================================================================ */

void vPortSetupTimerInterrupt(uint32_t ulTickRateHz, uint32_t ulCPUClockHz)
{
    /* Calculate reload value for desired tick rate */
    uint32_t ulReloadValue = (ulCPUClockHz / ulTickRateHz) - 1;
    
    /* Configure SysTick */
    SYSTICK_CVR = 0;                            /* Clear current value */
    SYSTICK_RVR = ulReloadValue;                /* Set reload value */
    SYSTICK_CSR = 0x07;                         /* Enable: clksrc=core, tickint=1, enable=1 */
}

BaseType_t xPortStartScheduler(void)
{
    /* Set exception priorities */
    /* PendSV and SysTick at lowest priority */
    SCB_SHPR3 = (configKERNEL_INTERRUPT_PRIORITY << 16) |  /* PendSV */
                (configKERNEL_INTERRUPT_PRIORITY << 24);   /* SysTick */
    
    /* SVCall at highest priority */
    SCB_SHPR2 = (configSVCALL_INTERRUPT_PRIORITY << 24);
    
    /* Initialize critical nesting */
    ulCriticalNesting = 0;
    
    /* Mark scheduler as running */
    ulSchedulerRunning = 1;
    
    /* Start first task - triggers PendSV */
    vPortYield();
    
    /* Enable interrupts */
    portENABLE_INTERRUPTS();
    
    /* Should not reach here */
    return 0;
}

void vPortEndScheduler(void)
{
    /* Disable interrupts */
    portDISABLE_INTERRUPTS();
    
    /* Mark scheduler as stopped */
    ulSchedulerRunning = 0;
}

/* ============================================================================
 * Task Control
 * ============================================================================ */

void vPortSetCurrentTCB(OSEK_TCB_t *pxTCB)
{
    pxCurrentTCB = pxTCB;
}

OSEK_TCB_t *pxPortGetCurrentTCB(void)
{
    return pxCurrentTCB;
}

/* ============================================================================
 * Weak Scheduler Hook (to be implemented by kernel)
 * ============================================================================ */

__attribute__((weak)) void OSEK_ScheduleNextTask(void)
{
    /* Implemented by kernel */
}

__attribute__((weak)) void OSEK_SVCHandler(uint32_t *pulStackFrame, uint8_t ucSVCNumber)
{
    (void)pulStackFrame;
    (void)ucSVCNumber;
    /* Implemented by kernel */
}

__attribute__((weak)) void OSEK_IncrementCounter(CounterType CounterID)
{
    (void)CounterID;
    /* Implemented by kernel */
}
