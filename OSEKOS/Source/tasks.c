/*
 * OSEK Task Management - FRET Fuzzing Target
 * Scheduler, task activation, context switch
 */

#include "include/osek.h"

/*============================================================================
 * Global State
 *============================================================================*/

/* Global state - accessible by fuzzer */
Os_TaskDynType      Os_TaskDyn[OS_MAX_TASKS];
volatile TickType   Os_TickCounter = 0;
TaskType            Os_CurrentTask = NULL_PTR;
TaskType            Os_ReadyQueue[OS_MAX_PRIORITY];
uint8_t             Os_TaskCount = 0;

/* Internal state */
static uint64_t     Os_ReadyMask = 0;
static AppModeType  Os_AppMode = OSDEFAULTAPPMODE;

/*============================================================================
 * Critical Section (simplified for QEMU)
 *============================================================================*/

static volatile uint32_t Os_IntLock = 0;

void Os_EnterCritical(void) {
    Os_IntLock++;
}

void Os_ExitCritical(void) {
    if (Os_IntLock > 0) Os_IntLock--;
}

void Os_DisableAllInterrupts(void)  { Os_IntLock++; }
void Os_EnableAllInterrupts(void)   { if (Os_IntLock > 0) Os_IntLock--; }
void Os_SuspendAllInterrupts(void)  { Os_IntLock++; }
void Os_ResumeAllInterrupts(void)   { if (Os_IntLock > 0) Os_IntLock--; }
void Os_SuspendOSInterrupts(void)   { Os_IntLock++; }
void Os_ResumeOSInterrupts(void)    { if (Os_IntLock > 0) Os_IntLock--; }

/*============================================================================
 * Scheduler Helpers
 *============================================================================*/

static TaskType Os_GetHighestReady(void) {
    if (Os_ReadyMask == 0) return NULL_PTR;
    
    /* Find highest bit set */
    for (int p = OS_MAX_PRIORITY - 1; p >= 0; p--) {
        if (Os_ReadyMask & (1ULL << p)) {
            return Os_ReadyQueue[p];
        }
    }
    return NULL_PTR;
}

void Os_AddToReady(TaskType task) {
    if (task == NULL_PTR) return;
    Os_TaskDynType* dyn = &Os_TaskDyn[task->index];
    uint8_t prio = dyn->currentPriority;
    
    dyn->state = READY;
    Os_ReadyQueue[prio] = task;
    Os_ReadyMask |= (1ULL << prio);
}

static void Os_RemoveFromReady(TaskType task) {
    if (task == NULL_PTR) return;
    Os_TaskDynType* dyn = &Os_TaskDyn[task->index];
    uint8_t prio = dyn->currentPriority;
    
    if (Os_ReadyQueue[prio] == task) {
        Os_ReadyQueue[prio] = NULL_PTR;
        Os_ReadyMask &= ~(1ULL << prio);
    }
}

void Os_Dispatch(void) {
    TaskType next = Os_GetHighestReady();
    
    if (next != NULL_PTR && next != Os_CurrentTask) {
        if (Os_CurrentTask != NULL_PTR) {
            Os_TaskDynType* curr = &Os_TaskDyn[Os_CurrentTask->index];
            if (curr->state == RUNNING) {
                curr->state = READY;
            }
        }
        
        Os_RemoveFromReady(next);
        Os_TaskDynType* nextDyn = &Os_TaskDyn[next->index];
        nextDyn->state = RUNNING;
        Os_CurrentTask = next;
        
        /* Call task entry (simplified - no real context switch) */
        if (next->entry != NULL_PTR) {
            next->entry();
        }
    }
}

/*============================================================================
 * Task API Implementation
 *============================================================================*/

StatusType Os_ActivateTask(TaskType TaskID) {
    if (TaskID == NULL_PTR || TaskID->index >= OS_MAX_TASKS) {
        return E_OS_ID;
    }
    
    Os_TaskDynType* dyn = &Os_TaskDyn[TaskID->index];
    
    Os_EnterCritical();
    
    if (dyn->activationCount >= TaskID->maxActivations) {
        Os_ExitCritical();
        return E_OS_LIMIT;
    }
    
    dyn->activationCount++;
    
    if (dyn->state == SUSPENDED) {
        dyn->currentPriority = TaskID->basePriority;
        dyn->eventsSet = 0;
        dyn->eventsWaiting = 0;
        Os_AddToReady(TaskID);
    }
    
    Os_Dispatch();
    Os_ExitCritical();
    
    return E_OK;
}

StatusType Os_ChainTask(TaskType TaskID) {
    if (TaskID == NULL_PTR) return E_OS_ID;
    if (Os_CurrentTask == NULL_PTR) return E_OS_CALLEVEL;
    
    Os_TaskDynType* currDyn = &Os_TaskDyn[Os_CurrentTask->index];
    Os_TaskDynType* nextDyn = &Os_TaskDyn[TaskID->index];
    
    if (currDyn->resourcesHeld != 0) return E_OS_RESOURCE;
    if (TaskID != Os_CurrentTask && nextDyn->activationCount >= TaskID->maxActivations) {
        return E_OS_LIMIT;
    }
    
    Os_EnterCritical();
    
    /* Terminate current */
    currDyn->activationCount--;
    currDyn->state = (currDyn->activationCount > 0) ? READY : SUSPENDED;
    if (currDyn->state == READY) {
        currDyn->currentPriority = Os_CurrentTask->basePriority;
        Os_AddToReady(Os_CurrentTask);
    }
    
    /* Activate next */
    nextDyn->activationCount++;
    if (nextDyn->state == SUSPENDED) {
        nextDyn->currentPriority = TaskID->basePriority;
        Os_AddToReady(TaskID);
    }
    
    Os_CurrentTask = NULL_PTR;
    Os_Dispatch();
    Os_ExitCritical();
    
    return E_OK;
}

StatusType Os_Schedule(void) {
    if (Os_CurrentTask == NULL_PTR) return E_OS_CALLEVEL;
    
    Os_TaskDynType* dyn = &Os_TaskDyn[Os_CurrentTask->index];
    if (dyn->resourcesHeld != 0) return E_OS_RESOURCE;
    
    Os_EnterCritical();
    Os_AddToReady(Os_CurrentTask);
    Os_Dispatch();
    Os_ExitCritical();
    
    return E_OK;
}

StatusType Os_GetTaskID(TaskRefType TaskID) {
    if (TaskID == NULL_PTR) return E_OS_PARAM_POINTER;
    *TaskID = Os_CurrentTask;
    return E_OK;
}

StatusType Os_GetTaskState(TaskType TaskID, TaskStateRefType State) {
    if (TaskID == NULL_PTR) return E_OS_ID;
    if (State == NULL_PTR) return E_OS_PARAM_POINTER;
    *State = Os_TaskDyn[TaskID->index].state;
    return E_OK;
}

/*============================================================================
 * OS Control
 *============================================================================*/

void Os_StartOS(AppModeType Mode) {
    Os_AppMode = Mode;
    StartupHook();
    
    /* Activate autostart tasks would go here */
    
    /* Run scheduler forever */
    while (1) {
        Os_Dispatch();
    }
}

void Os_ShutdownOS(StatusType Error) {
    ShutdownHook(Error);
    while (1) { /* halt */ }
}

AppModeType Os_GetActiveApplicationMode(void) {
    return Os_AppMode;
}

/*============================================================================
 * Weak Hook Defaults
 *============================================================================*/

__attribute__((weak)) void ErrorHook(StatusType Error) { (void)Error; }
__attribute__((weak)) void StartupHook(void) { }
__attribute__((weak)) void ShutdownHook(StatusType Error) { (void)Error; }
__attribute__((weak)) void PreTaskHook(void) { }
__attribute__((weak)) void PostTaskHook(void) { }
