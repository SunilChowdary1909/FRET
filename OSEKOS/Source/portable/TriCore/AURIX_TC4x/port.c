/*
 * OSEK Port Layer for AURIX TC4x (TriCore Architecture)
 * Context switching, interrupt handling, and timer setup
 */

#include "../../include/osek.h"
#include "../../include/osek_types.h"
#include "portmacro.h"

/*============================================================================
 * TriCore Architecture Specifics
 *============================================================================*/

/* TriCore uses a Context Save Area (CSA) mechanism for context switching.
 * Each context is saved in a linked list of CSA frames.
 * 
 * Key registers:
 * - PCXI: Previous Context Information (link to saved context)
 * - PSW: Program Status Word
 * - PC: Program Counter
 * - A[10]: Stack Pointer
 * - FCX: Free Context List pointer
 * - LCX: Last Context pointer (for overflow detection)
 */

/*============================================================================
 * Memory-Mapped Registers (TC4x specific)
 *============================================================================*/

/* System Control Unit (SCU) */
#define SCU_BASE            0xF0036000U
#define SCU_WDTCPU0CON0     (*(volatile uint32_t*)(SCU_BASE + 0x100U))
#define SCU_WDTSCON0        (*(volatile uint32_t*)(SCU_BASE + 0x0F0U))

/* Interrupt Router (IR) */
#define IR_BASE             0xF0037000U

/* System Timer (STM) - used for OS tick */
#define STM0_BASE           0xF0001000U
#define STM0_TIM0           (*(volatile uint32_t*)(STM0_BASE + 0x10U))
#define STM0_CAP            (*(volatile uint32_t*)(STM0_BASE + 0x2CU))
#define STM0_CMP0           (*(volatile uint32_t*)(STM0_BASE + 0x30U))
#define STM0_CMCON          (*(volatile uint32_t*)(STM0_BASE + 0x38U))
#define STM0_ICR            (*(volatile uint32_t*)(STM0_BASE + 0x3CU))
#define STM0_ISCR           (*(volatile uint32_t*)(STM0_BASE + 0x40U))

/* Service Request Nodes (SRN) */
#define SRC_STM0SR0         (*(volatile uint32_t*)(0xF0038490U))

/* Core Special Function Registers */
#define CORE_ID             __mfcr(0xFE1CU)

/*============================================================================
 * Inline Assembly Helpers
 *============================================================================*/

/* Move From Core Register */
static inline uint32_t __mfcr(uint32_t csfr) {
    uint32_t result;
    __asm__ volatile ("mfcr %0, %1" : "=d"(result) : "i"(csfr));
    return result;
}

/* Move To Core Register */
static inline void __mtcr(uint32_t csfr, uint32_t value) {
    __asm__ volatile ("mtcr %0, %1" : : "i"(csfr), "d"(value));
    __asm__ volatile ("isync");
}

/* Instruction Synchronization */
static inline void __isync(void) {
    __asm__ volatile ("isync");
}

/* Data Synchronization */
static inline void __dsync(void) {
    __asm__ volatile ("dsync");
}

/* Disable interrupts and return previous state */
static inline uint32_t __disable_interrupts(void) {
    uint32_t prev = __mfcr(0xFE04U); /* ICR - Interrupt Control Register */
    __mtcr(0xFE04U, prev & ~0x100U); /* Clear IE bit */
    return prev;
}

/* Restore interrupt state */
static inline void __restore_interrupts(uint32_t state) {
    __mtcr(0xFE04U, state);
}

/* Enable interrupts */
static inline void __enable_interrupts(void) {
    uint32_t icr = __mfcr(0xFE04U);
    __mtcr(0xFE04U, icr | 0x100U); /* Set IE bit */
}

/* Trigger software interrupt for context switch */
static inline void __syscall(uint32_t tin) {
    __asm__ volatile ("syscall %0" : : "i"(tin));
}

/*============================================================================
 * Critical Section Management
 *============================================================================*/

static volatile uint32_t Os_SavedInterruptState;
static volatile uint32_t Os_CriticalNesting = 0;

void Os_EnterCritical(void) {
    uint32_t state = __disable_interrupts();
    
    if (Os_CriticalNesting == 0) {
        Os_SavedInterruptState = state;
    }
    Os_CriticalNesting++;
}

void Os_ExitCritical(void) {
    if (Os_CriticalNesting > 0) {
        Os_CriticalNesting--;
        if (Os_CriticalNesting == 0) {
            __restore_interrupts(Os_SavedInterruptState);
        }
    }
}

void DisableAllInterrupts(void) {
    __disable_interrupts();
}

void EnableAllInterrupts(void) {
    __enable_interrupts();
}

static uint32_t Os_SuspendNesting = 0;
static uint32_t Os_SuspendedState;

void SuspendAllInterrupts(void) {
    uint32_t state = __disable_interrupts();
    if (Os_SuspendNesting == 0) {
        Os_SuspendedState = state;
    }
    Os_SuspendNesting++;
}

void ResumeAllInterrupts(void) {
    if (Os_SuspendNesting > 0) {
        Os_SuspendNesting--;
        if (Os_SuspendNesting == 0) {
            __restore_interrupts(Os_SuspendedState);
        }
    }
}

void SuspendOSInterrupts(void) {
    SuspendAllInterrupts();
}

void ResumeOSInterrupts(void) {
    ResumeAllInterrupts();
}

/*============================================================================
 * Context Switching (TriCore CSA mechanism)
 *============================================================================*/

/* PCXI register format */
#define PCXI_PCPN_MASK  0x00FF0000U  /* Previous CPU Priority Number */
#define PCXI_PIE_MASK   0x00000100U  /* Previous Interrupt Enable */
#define PCXI_UL_MASK    0x00000040U  /* Upper/Lower context indicator */
#define PCXI_PCXO_MASK  0x0000FFFFU  /* Previous Context Pointer Offset */
#define PCXI_PCXS_MASK  0x000F0000U  /* Previous Context Pointer Segment */

/* Get CSA address from PCXI value */
static inline uint32_t* Os_GetCSAAddress(uint32_t pcxi) {
    uint32_t segment = (pcxi & 0x000F0000U) << 12;
    uint32_t offset = (pcxi & 0x0000FFFFU) << 6;
    return (uint32_t*)(segment | offset);
}

/* Request a context switch (called from scheduler) */
void Os_RequestContextSwitch(void) {
    /* Trigger trap for context switch */
    __syscall(OS_SYSCALL_CONTEXT_SWITCH);
}

/* Initialize task context for first run */
void Os_InitializeTaskContext(Os_TCB* task) {
    /* Get a free CSA from the free list */
    uint32_t fcx = __mfcr(0xFE38U); /* FCX register */
    
    if (fcx == 0) {
        /* No free CSA - system error */
        ShutdownOS(E_OS_LIMIT);
        return;
    }
    
    uint32_t* csa = Os_GetCSAAddress(fcx);
    
    /* Update FCX to next free CSA */
    uint32_t next_fcx = csa[0];
    __mtcr(0xFE38U, next_fcx);
    
    /* Initialize upper context */
    csa[0] = 0;                              /* PCXI (will be updated) */
    csa[1] = (uint32_t)task->stackTop;       /* PSW - initial value */
    csa[2] = (uint32_t)task->stackTop;       /* A[10] - SP */
    csa[3] = (uint32_t)task->stackTop;       /* A[11] - Return address (unused) */
    csa[4] = 0;                              /* D[8] */
    csa[5] = 0;                              /* D[9] */
    csa[6] = 0;                              /* D[10] */
    csa[7] = 0;                              /* D[11] */
    csa[8] = 0;                              /* A[12] */
    csa[9] = 0;                              /* A[13] */
    csa[10] = 0;                             /* A[14] */
    csa[11] = 0;                             /* A[15] */
    csa[12] = 0;                             /* D[12] */
    csa[13] = 0;                             /* D[13] */
    csa[14] = 0;                             /* D[14] */
    csa[15] = 0;                             /* D[15] */
    
    /* Get another CSA for lower context */
    uint32_t fcx2 = __mfcr(0xFE38U);
    if (fcx2 == 0) {
        ShutdownOS(E_OS_LIMIT);
        return;
    }
    
    uint32_t* csa2 = Os_GetCSAAddress(fcx2);
    uint32_t next_fcx2 = csa2[0];
    __mtcr(0xFE38U, next_fcx2);
    
    /* Link upper to lower context */
    csa[0] = fcx2 | PCXI_UL_MASK;
    
    /* Initialize lower context */
    csa2[0] = 0;                             /* PCXI (end of chain) */
    csa2[1] = (uint32_t)task->entryPoint;    /* A[11] - Return address (PC) */
    csa2[2] = 0;                             /* A[2] */
    csa2[3] = 0;                             /* A[3] */
    csa2[4] = 0;                             /* D[0] */
    csa2[5] = 0;                             /* D[1] */
    csa2[6] = 0;                             /* D[2] */
    csa2[7] = 0;                             /* D[3] */
    csa2[8] = 0;                             /* A[4] */
    csa2[9] = 0;                             /* A[5] */
    csa2[10] = 0;                            /* A[6] */
    csa2[11] = 0;                            /* A[7] */
    csa2[12] = 0;                            /* D[4] */
    csa2[13] = 0;                            /* D[5] */
    csa2[14] = 0;                            /* D[6] */
    csa2[15] = 0;                            /* D[7] */
    
    /* Store PCXI in TCB */
    task->pcxi = fcx | PCXI_PIE_MASK;
    task->pc = (uint32_t)task->entryPoint;
    task->psw = 0x00000B80U;  /* User mode, interrupts enabled */
}

/* Context switch handler (called from trap/syscall) */
void Os_ContextSwitchHandler(void) {
    Os_TCB* currentTask = Os_Ocb.currentTask;
    Os_TCB* nextTask;
    
    /* Save current task's context if running */
    if (currentTask != NULL && currentTask->state == RUNNING) {
        currentTask->pcxi = __mfcr(0xFE00U);  /* Save PCXI */
        currentTask->state = READY;
        Os_AddToReadyQueue(currentTask);
    }
    
    /* Get highest priority ready task */
    nextTask = Os_GetHighestPriorityTask();
    
    if (nextTask == NULL) {
        /* No task ready - idle */
        Os_Ocb.currentTask = NULL;
        return;
    }
    
    /* Remove from ready queue and set as running */
    Os_RemoveFromReadyQueue(nextTask);
    nextTask->state = RUNNING;
    nextTask->startTime = Os_Ocb.tickCounter;
    Os_Ocb.currentTask = nextTask;
    
    /* Restore next task's context */
    __mtcr(0xFE00U, nextTask->pcxi);  /* Restore PCXI */
    
    Os_Ocb.contextSwitchNeeded = false;
}

/*============================================================================
 * System Timer (STM) for OS Tick
 *============================================================================*/

#define OS_TICK_FREQUENCY_HZ    1000U   /* 1ms tick */
#define OS_STM_FREQUENCY_HZ     100000000U  /* 100MHz STM clock (typical) */
#define OS_TICK_RELOAD_VALUE    (OS_STM_FREQUENCY_HZ / OS_TICK_FREQUENCY_HZ)

void Os_InitTimer(void) {
    /* Configure STM compare register for periodic interrupt */
    STM0_CMP0 = STM0_TIM0 + OS_TICK_RELOAD_VALUE;
    
    /* Configure compare match: 32-bit compare, reset on match */
    STM0_CMCON = 0x0000001FU;  /* Match on bits [31:0] */
    
    /* Enable compare interrupt */
    STM0_ICR = 0x00000001U;
    
    /* Configure Service Request Node */
    /* Enable, priority, type of service (CPU0) */
    SRC_STM0SR0 = 0x00001401U;  /* Enable, priority 1, CPU0 */
}

/* STM Interrupt Handler */
void Os_TickHandler(void) {
    /* Clear interrupt flag */
    STM0_ISCR = 0x00000001U;
    
    /* Update compare for next tick */
    STM0_CMP0 += OS_TICK_RELOAD_VALUE;
    
    /* Increment OS tick counter */
    Os_Ocb.tickCounter++;
    
    /* Increment system counter (triggers alarms) */
    IncrementCounter(0);  /* Counter 0 is system counter */
    
    /* Check if preemption needed */
    if (Os_Ocb.contextSwitchNeeded) {
        Os_ContextSwitchHandler();
    }
}

/*============================================================================
 * OS Startup
 *============================================================================*/

void StartOS(AppModeType Mode) {
    Os_Ocb.appMode = Mode;
    Os_Ocb.osState = RUNNING;
    Os_Ocb.tickCounter = 0;
    Os_Ocb.isrNestingLevel = 0;
    Os_Ocb.criticalNesting = 0;
    Os_Ocb.schedulerLocked = false;
    Os_Ocb.contextSwitchNeeded = false;
    Os_Ocb.readyQueueMask = 0;
    
    /* Clear ready queues */
    for (int i = 0; i < OS_MAX_PRIORITY_LEVELS; i++) {
        Os_Ocb.readyQueue[i] = NULL;
    }
    
    /* Call startup hook */
    #ifdef OS_STARTUP_HOOK
    StartupHook();
    #endif
    
    /* Activate autostart tasks */
    for (uint32_t i = 0; i < Os_TaskCount; i++) {
        Os_TCB* task = &Os_TaskTable[i];
        if (task->autostart && (task->autostartModes & (1U << Mode))) {
            task->activationCount = 1;
            task->releaseTime = 0;
            task->currentPriority = task->basePriority;
            Os_InitializeTaskContext(task);
            Os_AddToReadyQueue(task);
        }
    }
    
    /* Initialize system timer */
    Os_InitTimer();
    
    /* Enable interrupts */
    __enable_interrupts();
    
    /* Start first task */
    Os_Ocb.currentTask = Os_GetHighestPriorityTask();
    if (Os_Ocb.currentTask != NULL) {
        Os_RemoveFromReadyQueue(Os_Ocb.currentTask);
        Os_Ocb.currentTask->state = RUNNING;
        Os_Ocb.currentTask->startTime = 0;
        
        /* Jump to first task (never returns) */
        __mtcr(0xFE00U, Os_Ocb.currentTask->pcxi);
        __asm__ volatile ("rfe");  /* Return from exception - starts task */
    }
    
    /* Should never reach here */
    while (1) {
        __asm__ volatile ("wait");
    }
}

void ShutdownOS(StatusType Error) {
    __disable_interrupts();
    
    #ifdef OS_SHUTDOWN_HOOK
    ShutdownHook(Error);
    #endif
    
    /* Infinite loop */
    while (1) {
        __asm__ volatile ("wait");
    }
}

AppModeType GetActiveApplicationMode(void) {
    return Os_Ocb.appMode;
}

/*============================================================================
 * Tick Counter Access (for fuzzer integration)
 *============================================================================*/

volatile TickType Os_TickCounter;

TickType Os_GetTickCount(void) {
    return Os_Ocb.tickCounter;
}
