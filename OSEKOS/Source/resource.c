/*
 * OSEK Resource Management - FRET Fuzzing Target
 * Priority Ceiling Protocol
 */

#include "include/osek.h"

/*============================================================================
 * Global State - accessible by fuzzer
 *============================================================================*/

Os_ResourceDynType Os_ResourceDyn[OS_MAX_RESOURCES];
uint8_t Os_ResourceCount = 0;

/* External from tasks.c */
extern Os_TaskDynType Os_TaskDyn[];

/* Get current task (implemented in tasks.c) */
static TaskType Os_GetCurrentTask(void) {
    TaskType task;
    Os_GetTaskID(&task);
    return task;
}

/*============================================================================
 * Resource API
 *============================================================================*/

StatusType Os_GetResource(ResourceType ResID) {
    if (ResID == NULL_PTR || ResID->index >= OS_MAX_RESOURCES) {
        return E_OS_ID;
    }
    
    TaskType task = Os_GetCurrentTask();
    if (task == NULL_PTR) return E_OS_CALLEVEL;
    
    Os_ResourceDynType* res = &Os_ResourceDyn[ResID->index];
    Os_TaskDynType* dyn = &Os_TaskDyn[task->index];
    
    /* Already occupied? */
    if (res->isOccupied) return E_OS_ACCESS;
    
    /* Ceiling must be >= task priority */
    if (ResID->ceilingPriority < task->basePriority) {
        return E_OS_ACCESS;
    }
    
    Os_EnterCritical();
    
    /* Save state and raise priority */
    res->prevPriority = dyn->currentPriority;
    res->owner = task;
    res->isOccupied = TRUE;
    
    if (ResID->ceilingPriority > dyn->currentPriority) {
        dyn->currentPriority = ResID->ceilingPriority;
    }
    
    dyn->resourcesHeld |= (1U << ResID->index);
    
    Os_ExitCritical();
    return E_OK;
}

StatusType Os_ReleaseResource(ResourceType ResID) {
    if (ResID == NULL_PTR || ResID->index >= OS_MAX_RESOURCES) {
        return E_OS_ID;
    }
    
    TaskType task = Os_GetCurrentTask();
    if (task == NULL_PTR) return E_OS_CALLEVEL;
    
    Os_ResourceDynType* res = &Os_ResourceDyn[ResID->index];
    Os_TaskDynType* dyn = &Os_TaskDyn[task->index];
    
    /* Must be owner */
    if (!res->isOccupied || res->owner != task) {
        return E_OS_NOFUNC;
    }
    
    Os_EnterCritical();
    
    /* Restore priority */
    dyn->currentPriority = res->prevPriority;
    dyn->resourcesHeld &= ~(1U << ResID->index);
    
    res->isOccupied = FALSE;
    res->owner = NULL_PTR;
    
    Os_ExitCritical();
    
    /* May need to reschedule */
    Os_Schedule();
    
    return E_OK;
}
