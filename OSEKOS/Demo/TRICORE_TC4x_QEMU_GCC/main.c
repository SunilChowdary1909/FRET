/*
 * RTA-OS Demo Main Entry Point
 * Based on ETAS RTA-OS 5.0.2 patterns
 * Target: AURIX TC4x (TriCore) on QEMU
 */

#include "osek.h"
#include "osek_types.h"

/*============================================================================
 * Demo Selection
 *============================================================================*/

#if defined(mainCREATE_WATERS_DEMO) && mainCREATE_WATERS_DEMO == 1
    extern void main_waters(void);
    #define DEMO_MAIN main_waters
#elif defined(mainCREATE_COPTER_DEMO) && mainCREATE_COPTER_DEMO == 1
    extern void main_copter(void);
    #define DEMO_MAIN main_copter
#elif defined(mainCREATE_BLINKY_DEMO) && mainCREATE_BLINKY_DEMO == 1
    extern void main_blinky(void);
    #define DEMO_MAIN main_blinky
#else
    extern void main_blinky(void);
    #define DEMO_MAIN main_blinky
#endif

/*============================================================================
 * Fuzzer Integration
 *============================================================================*/

#ifdef FUZZ_ENABLED
/* Fuzz input buffer - accessed by fuzzer */
volatile uint8_t FUZZ_INPUT[4096] __attribute__((section(".fuzz_input")));

/* Current read position in fuzz buffer */
volatile uint32_t fuzz_input_offset = 0;

/* Signal job completion for timing analysis */
void trigger_job_done(void) {
    /* Empty - hooked by fuzzer */
}
#endif

/*============================================================================
 * Console Output (Semihosting)
 *============================================================================*/

/* Simple semihosting write for debug output */
void console_print(const char* str) {
    /* TriCore semihosting - simplified */
    (void)str;
}

/*============================================================================
 * Hardware Initialization
 *============================================================================*/

static void hardware_init(void) {
    /* Disable watchdog timers */
    /* Configure clocks */
    /* Setup memory protection if needed */
    
    /* For QEMU, most of this is not needed */
}

/*============================================================================
 * RTA-OS Hook Implementations (Callouts)
 * These match the signatures in the actual RTA-OS Os.h
 *============================================================================*/

/*
 * StartupHook - Called before OS starts scheduling
 * [$UKS 24]
 */
FUNC(void, OS_CALLOUT_CODE) StartupHook(void)
{
    console_print("RTA-OS StartupHook\n");
}

/*
 * ShutdownHook - Called when ShutdownOS is invoked
 * [$UKS 18]
 */
FUNC(void, OS_CALLOUT_CODE) ShutdownHook(StatusType Error)
{
    console_print("RTA-OS ShutdownHook\n");
    (void)Error;
}

/*
 * PreTaskHook - Called before each task runs
 * [$UKS 175] [$UKS 179]
 */
FUNC(void, OS_CALLOUT_CODE) PreTaskHook(void)
{
    /* Called before each task runs */
#ifdef FUZZ_ENABLED
    /* Record task start time for timing analysis */
#endif
}

/*
 * PostTaskHook - Called after each task completes
 * [$UKS 176] [$UKS 180]
 */
FUNC(void, OS_CALLOUT_CODE) PostTaskHook(void)
{
    /* Called after each task completes */
#ifdef FUZZ_ENABLED
    /* Trigger job completion notification */
    trigger_job_done();
#endif
}

/*
 * ErrorHook - Called when an error occurs
 * [$UKS 479]
 */
FUNC(void, OS_CALLOUT_CODE) ErrorHook(StatusType Error)
{
    console_print("RTA-OS ErrorHook\n");
    (void)Error;
    
    /* In a real system, log the error and possibly take corrective action */
}

/*
 * ProtectionHook - Called on protection violation
 * Returns action to take (PRO_IGNORE, PRO_TERMINATETASKISR, etc.)
 */
FUNC(ProtectionReturnType, OS_CALLOUT_CODE) ProtectionHook(StatusType FatalError)
{
    console_print("RTA-OS ProtectionHook\n");
    (void)FatalError;
    
    /* PRO_SHUTDOWN = 0 - Shutdown the OS */
    return 0;
}

/*
 * Os_Cbk_Idle - Called when no task is ready to run
 * [$UKS 161]
 * Returns: TRUE if idle processing is complete, FALSE to continue
 */
FUNC(uint8_t, OS_CALLOUT_CODE) Os_Cbk_Idle(void)
{
    /* Idle processing - could enter low power mode */
    return FALSE;  /* Continue idling */
}

/*
 * Os_Cbk_StackOverrunHook - Called on stack overflow detection
 */
FUNC(void, OS_CALLOUT_CODE) Os_Cbk_StackOverrunHook(Os_StackSizeType Overrun, Os_StackOverrunType Reason)
{
    console_print("RTA-OS Stack Overrun!\n");
    (void)Overrun;
    (void)Reason;
}

/*
 * Os_Cbk_GetStopwatch - Returns current stopwatch value for timing
 * [$UKS 536]
 */
FUNC(Os_StopwatchTickType, OS_CALLOUT_CODE) Os_Cbk_GetStopwatch(void)
{
    /* Return current timer value - used for execution time measurement */
    return Os_TickCounter;
}

/*
 * Os_Cbk_TimeOverrunHook - Called when execution budget exceeded
 * [$UKS 537]
 */
FUNC(void, OS_CALLOUT_CODE) Os_Cbk_TimeOverrunHook(Os_StopwatchTickType Overrun)
{
    console_print("RTA-OS Time Overrun!\n");
    (void)Overrun;
}

/*============================================================================
 * Global OS Variables
 *============================================================================*/

/* OS Tick Counter */
volatile TickType Os_TickCounter = 0;

/* Current application mode */
volatile AppModeType Os_CurrentAppMode = OSDEFAULTAPPMODE;

/*============================================================================
 * Main Entry Point
 *============================================================================*/

int main(void)
{
    /* Initialize hardware */
    hardware_init();
    
    console_print("RTA-OS Demo Starting\n");
    
    /* Run the selected demo */
    DEMO_MAIN();
    
    /* Should not reach here - StartOS doesn't return */
    console_print("ERROR: Demo returned unexpectedly\n");
    
    while (1) {
        /* Halt */
    }
    
    return 0;
}

/*============================================================================
 * Multi-Core Support Stubs (for future use)
 *============================================================================*/

/* Per-core controlled state */
Os_ControlledCoreType Os_ControlledCoreInfo0;
Os_ControlledCoreType Os_ControlledCoreInfo1;
Os_ControlledCoreType Os_ControlledCoreInfo2;
Os_ControlledCoreType Os_ControlledCoreInfo3;
Os_ControlledCoreType Os_ControlledCoreInfo4;
Os_ControlledCoreType Os_ControlledCoreInfo5;

/* Total number of cores */
const CoreIdType Os_TotalNumberOfCores = 6;

/*============================================================================
 * OS API Stub Implementations
 * These are minimal stubs - real implementation would be in Os_Kernel.c
 *============================================================================*/

/* Get current core ID */
FUNC(CoreIdType, OS_CODE) GetCoreID(void)
{
    /* For single-core demo, always return 0 */
    return 0;
}

/* Critical section */
void Os_EnterCritical(void)
{
    DisableAllInterrupts();
}

void Os_ExitCritical(void)
{
    EnableAllInterrupts();
}

/* Get tick count */
TickType Os_GetTickCount(void)
{
    return Os_TickCounter;
}
