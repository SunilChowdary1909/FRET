/*
 * OSEK Alarm/Counter Management - FRET Fuzzing Target
 */

#include "include/osek.h"

/*============================================================================
 * Alarm/Counter Data - accessible by fuzzer
 *============================================================================*/

Os_AlarmDynType Os_AlarmDyn[OS_MAX_ALARMS];
Os_CounterDynType Os_CounterDyn[OS_MAX_COUNTERS];
uint8_t Os_AlarmCount = 0;
uint8_t Os_CounterCount = 0;

/* Static alarm configurations - set by application */
static const Os_AlarmType* Os_AlarmCfg[OS_MAX_ALARMS];

/* External: task activation from tasks.c */
extern StatusType Os_ActivateTask(TaskType TaskID);
extern StatusType Os_SetEvent(TaskType TaskID, EventMaskType Mask);

/*============================================================================
 * Counter API
 *============================================================================*/

StatusType Os_IncrementCounter(CounterType CounterID) {
    if (CounterID == NULL_PTR) return E_OS_ID;
    
    Os_CounterDynType* cnt = &Os_CounterDyn[CounterID->index];
    
    Os_EnterCritical();
    
    cnt->value++;
    if (cnt->value > CounterID->maxAllowedValue) {
        cnt->value = 0;
    }
    
    /* Check alarms attached to this counter */
    for (uint8 i = 0; i < Os_AlarmCount; i++) {
        Os_AlarmDynType* alm = &Os_AlarmDyn[i];
        const Os_AlarmType* cfg = Os_AlarmCfg[i];
        
        if (cfg == NULL_PTR) continue;
        if (!alm->isActive) continue;
        if (cfg->counter != CounterID) continue;
        
        if (alm->expireTime == cnt->value) {
            /* Fire alarm action */
            switch (cfg->actionType) {
                case OS_ALARM_ACTION_ACTIVATETASK:
                    Os_ActivateTask(cfg->action.taskId);
                    break;
                case OS_ALARM_ACTION_SETEVENT:
                    Os_SetEvent(cfg->action.setEvent.taskId, cfg->action.setEvent.event);
                    break;
                case OS_ALARM_ACTION_CALLBACK:
                    if (cfg->action.callback) cfg->action.callback();
                    break;
            }
            
            /* Reload or deactivate */
            if (alm->cycle > 0) {
                alm->expireTime = cnt->value + alm->cycle;
                if (alm->expireTime > CounterID->maxAllowedValue) {
                    alm->expireTime -= (CounterID->maxAllowedValue + 1);
                }
            } else {
                alm->isActive = FALSE;
            }
        }
    }
    
    Os_ExitCritical();
    return E_OK;
}

StatusType Os_GetCounterValue(CounterType CounterID, TickRefType Value) {
    if (CounterID == NULL_PTR) return E_OS_ID;
    if (Value == NULL_PTR) return E_OS_PARAM_POINTER;
    
    *Value = Os_CounterDyn[CounterID->index].value;
    return E_OK;
}

/*============================================================================
 * Alarm API
 *============================================================================*/

StatusType Os_SetRelAlarm(AlarmType AlarmID, TickType increment, TickType cycle) {
    if (AlarmID == NULL_PTR) return E_OS_ID;
    
    Os_AlarmDynType* alm = &Os_AlarmDyn[AlarmID->index];
    CounterType cnt = AlarmID->counter;
    
    if (alm->isActive) return E_OS_STATE;
    if (increment == 0 || increment > cnt->maxAllowedValue) return E_OS_VALUE;
    if (cycle != 0 && (cycle < cnt->minCycle || cycle > cnt->maxAllowedValue)) return E_OS_VALUE;
    
    Os_EnterCritical();
    
    TickType now = Os_CounterDyn[cnt->index].value;
    alm->expireTime = now + increment;
    if (alm->expireTime > cnt->maxAllowedValue) {
        alm->expireTime -= (cnt->maxAllowedValue + 1);
    }
    alm->cycle = cycle;
    alm->isActive = TRUE;
    
    /* Store config reference */
    Os_AlarmCfg[AlarmID->index] = AlarmID;
    
    Os_ExitCritical();
    return E_OK;
}

StatusType Os_SetAbsAlarm(AlarmType AlarmID, TickType start, TickType cycle) {
    if (AlarmID == NULL_PTR) return E_OS_ID;
    
    Os_AlarmDynType* alm = &Os_AlarmDyn[AlarmID->index];
    CounterType cnt = AlarmID->counter;
    
    if (alm->isActive) return E_OS_STATE;
    if (start > cnt->maxAllowedValue) return E_OS_VALUE;
    if (cycle != 0 && (cycle < cnt->minCycle || cycle > cnt->maxAllowedValue)) return E_OS_VALUE;
    
    Os_EnterCritical();
    
    alm->expireTime = start;
    alm->cycle = cycle;
    alm->isActive = TRUE;
    
    /* Store config reference */
    Os_AlarmCfg[AlarmID->index] = AlarmID;
    
    Os_ExitCritical();
    return E_OK;
}

StatusType Os_CancelAlarm(AlarmType AlarmID) {
    if (AlarmID == NULL_PTR) return E_OS_ID;
    
    Os_AlarmDynType* alm = &Os_AlarmDyn[AlarmID->index];
    
    if (!alm->isActive) return E_OS_NOFUNC;
    
    Os_EnterCritical();
    alm->isActive = FALSE;
    Os_ExitCritical();
    
    return E_OK;
}

StatusType Os_GetAlarm(AlarmType AlarmID, TickRefType Tick) {
    if (AlarmID == NULL_PTR) return E_OS_ID;
    if (Tick == NULL_PTR) return E_OS_PARAM_POINTER;
    
    Os_AlarmDynType* alm = &Os_AlarmDyn[AlarmID->index];
    
    if (!alm->isActive) return E_OS_NOFUNC;
    
    CounterType cnt = AlarmID->counter;
    TickType now = Os_CounterDyn[cnt->index].value;
    
    if (alm->expireTime >= now) {
        *Tick = alm->expireTime - now;
    } else {
        *Tick = (cnt->maxAllowedValue - now) + alm->expireTime + 1;
    }
    
    return E_OK;
}

StatusType Os_GetAlarmBase(AlarmType AlarmID, AlarmBaseRefType Info) {
    if (AlarmID == NULL_PTR) return E_OS_ID;
    if (Info == NULL_PTR) return E_OS_PARAM_POINTER;
    
    CounterType cnt = AlarmID->counter;
    Info->maxallowedvalue = cnt->maxAllowedValue;
    Info->ticksperbase = cnt->ticksPerBase;
    Info->mincycle = cnt->minCycle;
    
    return E_OK;
}
