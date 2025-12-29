/*
 * OSEK/RTA-OS API Header
 * Minimal implementation for FRET fuzzing
 * Target: AURIX TC4x (TriCore) on QEMU
 */

#ifndef OSEK_H
#define OSEK_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stddef.h>

/*============================================================================
 * Basic Types
 *============================================================================*/

typedef uint8_t     boolean;
typedef uint8_t     uint8;
typedef uint16_t    uint16;
typedef uint32_t    uint32;
typedef uint64_t    uint64;
typedef int8_t      sint8;
typedef int16_t     sint16;
typedef int32_t     sint32;

#ifndef TRUE
#define TRUE        1U
#endif
#ifndef FALSE
#define FALSE       0U
#endif
#ifndef NULL_PTR
#define NULL_PTR    ((void*)0)
#endif

/*============================================================================
 * AUTOSAR Macros (simplified)
 *============================================================================*/

#define FUNC(rettype, memclass)                 rettype
#define P2VAR(ptrtype, memclass, ptrclass)      ptrtype *
#define P2CONST(ptrtype, memclass, ptrclass)    const ptrtype *
#define CONSTP2CONST(ptrtype, m, p)             const ptrtype * const
#define CONST(type, memclass)                   const type
#define VAR(type, memclass)                     type
#define TYPEDEF
#define OS_CODE
#define OS_VAR
#define OS_CONST
#define OS_CALLOUT_CODE

/*============================================================================
 * OSEK Basic Types
 *============================================================================*/

typedef uint8_t     StatusType;
typedef uint32_t    AppModeType;
typedef uint32_t    TickType;
typedef uint32_t    EventMaskType;
typedef uint8_t     TaskStateType;
typedef uint8_t     CoreIdType;

/* Pointers */
typedef TickType*       TickRefType;
typedef EventMaskType*  EventMaskRefType;
typedef TaskStateType*  TaskStateRefType;

/*============================================================================
 * Status Codes
 *============================================================================*/

#define E_OK                    ((StatusType)0U)
#define E_OS_ACCESS             ((StatusType)1U)
#define E_OS_CALLEVEL           ((StatusType)2U)
#define E_OS_ID                 ((StatusType)3U)
#define E_OS_LIMIT              ((StatusType)4U)
#define E_OS_NOFUNC             ((StatusType)5U)
#define E_OS_RESOURCE           ((StatusType)6U)
#define E_OS_STATE              ((StatusType)7U)
#define E_OS_VALUE              ((StatusType)8U)
#define E_OS_PARAM_POINTER      ((StatusType)9U)

/*============================================================================
 * Task States
 *============================================================================*/

#define SUSPENDED               ((TaskStateType)0U)
#define READY                   ((TaskStateType)1U)
#define WAITING                 ((TaskStateType)2U)
#define RUNNING                 ((TaskStateType)3U)

/*============================================================================
 * Application Modes
 *============================================================================*/

#define OSDEFAULTAPPMODE        ((AppModeType)0U)

/*============================================================================
 * Configuration Limits
 *============================================================================*/

#define OS_MAX_TASKS            32U
#define OS_MAX_RESOURCES        16U
#define OS_MAX_ALARMS           16U
#define OS_MAX_COUNTERS         4U
#define OS_MAX_PRIORITY         64U

/*============================================================================
 * Task Type (pointer-based like RTA-OS)
 *============================================================================*/

/* Task static configuration */
typedef struct Os_TaskType_s {
    uint8_t     index;
    uint8_t     basePriority;
    uint8_t     maxActivations;
    boolean     autostart;
    uint32_t    stackSize;
    void        (*entry)(void);
} Os_TaskType;

/* TaskType is pointer to config (RTA-OS style) */
typedef const Os_TaskType* TaskType;
typedef TaskType* TaskRefType;

#define INVALID_TASK    ((TaskType)NULL_PTR)

/* Task runtime state */
typedef struct {
    TaskStateType   state;
    uint8_t         currentPriority;
    uint8_t         activationCount;
    uint8_t         _pad;           /* Explicit padding for alignment */
    EventMaskType   eventsSet;
    EventMaskType   eventsWaiting;
    uint32_t        resourcesHeld;
} Os_TaskDynType;

/*============================================================================
 * Resource Type
 *============================================================================*/

typedef struct Os_ResourceType_s {
    uint8_t     index;
    uint8_t     ceilingPriority;
} Os_ResourceType;

typedef const Os_ResourceType* ResourceType;

typedef struct {
    TaskType    owner;          /* 4 bytes (pointer) */
    uint8_t     prevPriority;   /* 1 byte */
    boolean     isOccupied;     /* 1 byte */
    uint8_t     _pad[2];        /* 2 bytes padding */
} Os_ResourceDynType;

/*============================================================================
 * Counter Type
 *============================================================================*/

typedef struct Os_CounterType_s {
    uint8_t     index;          /* 1 byte */
    uint8_t     _pad[3];        /* 3 bytes padding */
    TickType    maxAllowedValue;
    TickType    ticksPerBase;
    TickType    minCycle;
} Os_CounterType;

typedef const Os_CounterType* CounterType;

typedef struct {
    TickType    value;
} Os_CounterDynType;

/* Alarm base info */
typedef struct {
    TickType    maxallowedvalue;
    TickType    ticksperbase;
    TickType    mincycle;
} AlarmBaseType;

typedef AlarmBaseType* AlarmBaseRefType;

/*============================================================================
 * Alarm Type
 *============================================================================*/

#define OS_ALARM_ACTION_ACTIVATETASK    0U
#define OS_ALARM_ACTION_SETEVENT        1U
#define OS_ALARM_ACTION_CALLBACK        2U

typedef struct Os_AlarmType_s {
    uint8_t         index;
    CounterType     counter;
    uint8_t         actionType;
    union {
        TaskType    taskId;
        struct {
            TaskType        taskId;
            EventMaskType   event;
        } setEvent;
        void (*callback)(void);
    } action;
} Os_AlarmType;

typedef const Os_AlarmType* AlarmType;

typedef struct {
    boolean     isActive;       /* 1 byte */
    uint8_t     _pad[3];        /* 3 bytes padding for alignment */
    TickType    expireTime;     /* 4 bytes */
    TickType    cycle;          /* 4 bytes */
} Os_AlarmDynType;

/*============================================================================
 * ISR Type (for completeness)
 *============================================================================*/

typedef void* ISRType;

/*============================================================================
 * Task API
 *============================================================================*/

extern StatusType Os_ActivateTask(TaskType TaskID);
extern StatusType Os_ChainTask(TaskType TaskID);
extern StatusType Os_Schedule(void);
extern StatusType Os_GetTaskID(TaskRefType TaskID);
extern StatusType Os_GetTaskState(TaskType TaskID, TaskStateRefType State);

/* RTA-OS Fast Termination */
#define TerminateTask()         return
#define ChainTask(x)            Os_ChainTask(x); return

/* Standard OSEK names */
#define ActivateTask            Os_ActivateTask
#define Schedule()              Os_Schedule()
#define GetTaskID               Os_GetTaskID
#define GetTaskState            Os_GetTaskState

/*============================================================================
 * Resource API
 *============================================================================*/

extern StatusType Os_GetResource(ResourceType ResID);
extern StatusType Os_ReleaseResource(ResourceType ResID);

#define GetResource             Os_GetResource
#define ReleaseResource         Os_ReleaseResource

/*============================================================================
 * Event API
 *============================================================================*/

extern StatusType Os_SetEvent(TaskType TaskID, EventMaskType Mask);
extern StatusType Os_ClearEvent(EventMaskType Mask);
extern StatusType Os_GetEvent(TaskType TaskID, EventMaskRefType Event);
extern StatusType Os_WaitEvent(EventMaskType Mask);

#define SetEvent                Os_SetEvent
#define ClearEvent              Os_ClearEvent
#define GetEvent                Os_GetEvent
#define WaitEvent               Os_WaitEvent

/*============================================================================
 * Alarm API
 *============================================================================*/

extern StatusType Os_GetAlarmBase(AlarmType AlarmID, AlarmBaseRefType Info);
extern StatusType Os_GetAlarm(AlarmType AlarmID, TickRefType Tick);
extern StatusType Os_SetRelAlarm(AlarmType AlarmID, TickType increment, TickType cycle);
extern StatusType Os_SetAbsAlarm(AlarmType AlarmID, TickType start, TickType cycle);
extern StatusType Os_CancelAlarm(AlarmType AlarmID);
extern StatusType Os_IncrementCounter(CounterType CounterID);

#define GetAlarmBase            Os_GetAlarmBase
#define GetAlarm                Os_GetAlarm
#define SetRelAlarm             Os_SetRelAlarm
#define SetAbsAlarm             Os_SetAbsAlarm
#define CancelAlarm             Os_CancelAlarm

/*============================================================================
 * Interrupt API
 *============================================================================*/

extern void Os_DisableAllInterrupts(void);
extern void Os_EnableAllInterrupts(void);
extern void Os_SuspendAllInterrupts(void);
extern void Os_ResumeAllInterrupts(void);
extern void Os_SuspendOSInterrupts(void);
extern void Os_ResumeOSInterrupts(void);

#define DisableAllInterrupts    Os_DisableAllInterrupts
#define EnableAllInterrupts     Os_EnableAllInterrupts
#define SuspendAllInterrupts    Os_SuspendAllInterrupts
#define ResumeAllInterrupts     Os_ResumeAllInterrupts
#define SuspendOSInterrupts     Os_SuspendOSInterrupts
#define ResumeOSInterrupts      Os_ResumeOSInterrupts

/*============================================================================
 * OS Control
 *============================================================================*/

extern void Os_StartOS(AppModeType Mode);
extern void Os_ShutdownOS(StatusType Error);
extern AppModeType Os_GetActiveApplicationMode(void);

#define StartOS                 Os_StartOS
#define ShutdownOS              Os_ShutdownOS
#define GetActiveApplicationMode Os_GetActiveApplicationMode

/*============================================================================
 * Hooks (implement in your application)
 *============================================================================*/

extern void ErrorHook(StatusType Error);
extern void StartupHook(void);
extern void ShutdownHook(StatusType Error);
extern void PreTaskHook(void);
extern void PostTaskHook(void);

/*============================================================================
 * Task/ISR Definition Macros
 *============================================================================*/

#define TASK(name)              void Os_Entry_##name(void)
#define ISR(name)               void Os_Entry_##name(void)
#define DeclareTask(name)       extern const Os_TaskType name##_cfg; \
                                extern const TaskType name
#define DeclareResource(name)   extern const ResourceType name
#define DeclareAlarm(name)      extern const AlarmType name
#define DeclareEvent(name)      extern const EventMaskType name

/*============================================================================
 * Internal - used by kernel
 *============================================================================*/

extern Os_TaskDynType       Os_TaskDyn[OS_MAX_TASKS];
extern Os_ResourceDynType   Os_ResourceDyn[OS_MAX_RESOURCES];
extern Os_AlarmDynType      Os_AlarmDyn[OS_MAX_ALARMS];
extern Os_CounterDynType    Os_CounterDyn[OS_MAX_COUNTERS];
extern volatile TickType    Os_TickCounter;

void Os_EnterCritical(void);
void Os_ExitCritical(void);

#ifdef __cplusplus
}
#endif

#endif /* OSEK_H */
