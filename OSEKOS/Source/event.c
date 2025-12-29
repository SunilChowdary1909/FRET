/*
 * OSEK Event Management - FRET Fuzzing Target
 * For Extended Conformance Classes (ECC)
 */

#include "include/osek.h"

/*============================================================================
 * External from tasks.c
 *============================================================================*/

extern Os_TaskDynType Os_TaskDyn[];

static TaskType Os_GetCurrentTask(void) {
    TaskType task;
    Os_GetTaskID(&task);
    return task;
}

/* From tasks.c - add task to ready queue */
extern void Os_AddToReady(TaskType task);
extern void Os_Dispatch(void);

/*============================================================================
 * Event API
 *============================================================================*/

StatusType Os_SetEvent(TaskType TaskID, EventMaskType Mask) {
    if (TaskID == NULL_PTR || TaskID->index >= OS_MAX_TASKS) {
        return E_OS_ID;
    }
    
    Os_TaskDynType* dyn = &Os_TaskDyn[TaskID->index];
    
    if (dyn->state == SUSPENDED) {
        return E_OS_STATE;
    }
    
    Os_EnterCritical();
    
    dyn->eventsSet |= Mask;
    
    /* Wake up if waiting for this event */
    if (dyn->state == WAITING) {
        if ((dyn->eventsSet & dyn->eventsWaiting) != 0) {
            dyn->state = READY;
            Os_AddToReady(TaskID);
            Os_Dispatch();
        }
    }
    
    Os_ExitCritical();
    return E_OK;
}

StatusType Os_ClearEvent(EventMaskType Mask) {
    TaskType task = Os_GetCurrentTask();
    if (task == NULL_PTR) return E_OS_CALLEVEL;
    
    Os_EnterCritical();
    Os_TaskDyn[task->index].eventsSet &= ~Mask;
    Os_ExitCritical();
    
    return E_OK;
}

StatusType Os_GetEvent(TaskType TaskID, EventMaskRefType Event) {
    if (TaskID == NULL_PTR) return E_OS_ID;
    if (Event == NULL_PTR) return E_OS_PARAM_POINTER;
    
    Os_TaskDynType* dyn = &Os_TaskDyn[TaskID->index];
    if (dyn->state == SUSPENDED) return E_OS_STATE;
    
    *Event = dyn->eventsSet;
    return E_OK;
}

StatusType Os_WaitEvent(EventMaskType Mask) {
    TaskType task = Os_GetCurrentTask();
    if (task == NULL_PTR) return E_OS_CALLEVEL;
    
    Os_TaskDynType* dyn = &Os_TaskDyn[task->index];
    
    if (dyn->resourcesHeld != 0) return E_OS_RESOURCE;
    
    Os_EnterCritical();
    
    /* Already set? */
    if ((dyn->eventsSet & Mask) != 0) {
        Os_ExitCritical();
        return E_OK;
    }
    
    /* Block */
    dyn->eventsWaiting = Mask;
    dyn->state = WAITING;
    Os_Dispatch();
    
    Os_ExitCritical();
    return E_OK;
}
