/*
 * OSEK System Call Interface for AURIX TC4x
 * Provides syscall/trap handling for OSEK service calls
 */

#include "osek.h"
#include "portmacro.h"

/*
 * System call numbers for OSEK services
 * These are used with the SYSCALL instruction to invoke OS services
 */
#define SYSCALL_ACTIVATE_TASK       1
#define SYSCALL_TERMINATE_TASK      2
#define SYSCALL_CHAIN_TASK          3
#define SYSCALL_SCHEDULE            4
#define SYSCALL_GET_TASK_ID         5
#define SYSCALL_GET_TASK_STATE      6
#define SYSCALL_GET_RESOURCE        10
#define SYSCALL_RELEASE_RESOURCE    11
#define SYSCALL_SET_EVENT           20
#define SYSCALL_CLEAR_EVENT         21
#define SYSCALL_GET_EVENT           22
#define SYSCALL_WAIT_EVENT          23
#define SYSCALL_SET_REL_ALARM       30
#define SYSCALL_SET_ABS_ALARM       31
#define SYSCALL_CANCEL_ALARM        32
#define SYSCALL_GET_ALARM_BASE      33
#define SYSCALL_GET_ALARM           34
#define SYSCALL_SHUTDOWN_OS         99

/*
 * Syscall handler - called from trap 6 handler
 * This is the main dispatcher for all OSEK service calls
 *
 * Parameters are passed in registers:
 * - D4: syscall number
 * - D5-D8: arguments
 * 
 * Return value placed in D2
 */
StatusType OSEK_SyscallHandler(uint32_t syscall_num, uint32_t arg1, 
                                uint32_t arg2, uint32_t arg3, uint32_t arg4)
{
    StatusType status = E_OK;
    
    switch (syscall_num)
    {
        /* Task Management */
        case SYSCALL_ACTIVATE_TASK:
            status = ActivateTask((TaskType)arg1);
            break;
            
        case SYSCALL_TERMINATE_TASK:
            status = TerminateTask();
            break;
            
        case SYSCALL_CHAIN_TASK:
            status = ChainTask((TaskType)arg1);
            break;
            
        case SYSCALL_SCHEDULE:
            status = Schedule();
            break;
            
        case SYSCALL_GET_TASK_ID:
            status = GetTaskID((TaskRefType)arg1);
            break;
            
        case SYSCALL_GET_TASK_STATE:
            status = GetTaskState((TaskType)arg1, (TaskStateRefType)arg2);
            break;
            
        /* Resource Management */
        case SYSCALL_GET_RESOURCE:
            status = GetResource((ResourceType)arg1);
            break;
            
        case SYSCALL_RELEASE_RESOURCE:
            status = ReleaseResource((ResourceType)arg1);
            break;
            
        /* Event Control */
        case SYSCALL_SET_EVENT:
            status = SetEvent((TaskType)arg1, (EventMaskType)arg2);
            break;
            
        case SYSCALL_CLEAR_EVENT:
            status = ClearEvent((EventMaskType)arg1);
            break;
            
        case SYSCALL_GET_EVENT:
            status = GetEvent((TaskType)arg1, (EventMaskRefType)arg2);
            break;
            
        case SYSCALL_WAIT_EVENT:
            status = WaitEvent((EventMaskType)arg1);
            break;
            
        /* Alarm Management */
        case SYSCALL_SET_REL_ALARM:
            status = SetRelAlarm((AlarmType)arg1, (TickType)arg2, (TickType)arg3);
            break;
            
        case SYSCALL_SET_ABS_ALARM:
            status = SetAbsAlarm((AlarmType)arg1, (TickType)arg2, (TickType)arg3);
            break;
            
        case SYSCALL_CANCEL_ALARM:
            status = CancelAlarm((AlarmType)arg1);
            break;
            
        case SYSCALL_GET_ALARM_BASE:
            status = GetAlarmBase((AlarmType)arg1, (AlarmBaseRefType)arg2);
            break;
            
        case SYSCALL_GET_ALARM:
            status = GetAlarm((AlarmType)arg1, (TickRefType)arg2);
            break;
            
        /* System Shutdown */
        case SYSCALL_SHUTDOWN_OS:
            ShutdownOS((StatusType)arg1);
            /* Should not return */
            status = E_OS_SYS_ABORT;
            break;
            
        default:
            status = E_OS_SERVICEID;
            break;
    }
    
    return status;
}

/*
 * Trap 6 (Syscall) entry point
 * This is called from the assembly trap handler
 * 
 * Extracts parameters from saved context and calls the dispatcher
 */
void OSEK_Trap6Handler(void)
{
    uint32_t syscall_num;
    uint32_t arg1, arg2, arg3, arg4;
    StatusType result;
    
    /* Read syscall number and arguments from saved registers */
    /* On TriCore, D4-D8 contain parameters, D2 returns result */
    __asm__ volatile (
        "mov %0, %%d4\n\t"
        "mov %1, %%d5\n\t"
        "mov %2, %%d6\n\t"
        "mov %3, %%d7\n\t"
        "mov %4, %%d8"
        : "=d"(syscall_num), "=d"(arg1), "=d"(arg2), "=d"(arg3), "=d"(arg4)
    );
    
    /* Dispatch the syscall */
    result = OSEK_SyscallHandler(syscall_num, arg1, arg2, arg3, arg4);
    
    /* Store result in D2 (return register) */
    __asm__ volatile (
        "mov %%d2, %0"
        : 
        : "d"(result)
    );
}

/*
 * User-mode syscall wrapper functions
 * These can be called from user tasks to invoke OS services
 * without needing supervisor privileges
 */

static inline StatusType osek_syscall0(uint32_t num)
{
    StatusType result;
    __asm__ volatile (
        "mov %%d4, %1\n\t"
        "syscall 0\n\t"
        "mov %0, %%d2"
        : "=d"(result)
        : "d"(num)
        : "d4"
    );
    return result;
}

static inline StatusType osek_syscall1(uint32_t num, uint32_t a1)
{
    StatusType result;
    __asm__ volatile (
        "mov %%d4, %1\n\t"
        "mov %%d5, %2\n\t"
        "syscall 0\n\t"
        "mov %0, %%d2"
        : "=d"(result)
        : "d"(num), "d"(a1)
        : "d4", "d5"
    );
    return result;
}

static inline StatusType osek_syscall2(uint32_t num, uint32_t a1, uint32_t a2)
{
    StatusType result;
    __asm__ volatile (
        "mov %%d4, %1\n\t"
        "mov %%d5, %2\n\t"
        "mov %%d6, %3\n\t"
        "syscall 0\n\t"
        "mov %0, %%d2"
        : "=d"(result)
        : "d"(num), "d"(a1), "d"(a2)
        : "d4", "d5", "d6"
    );
    return result;
}

static inline StatusType osek_syscall3(uint32_t num, uint32_t a1, uint32_t a2, uint32_t a3)
{
    StatusType result;
    __asm__ volatile (
        "mov %%d4, %1\n\t"
        "mov %%d5, %2\n\t"
        "mov %%d6, %3\n\t"
        "mov %%d7, %4\n\t"
        "syscall 0\n\t"
        "mov %0, %%d2"
        : "=d"(result)
        : "d"(num), "d"(a1), "d"(a2), "d"(a3)
        : "d4", "d5", "d6", "d7"
    );
    return result;
}

/*
 * User-callable system call wrappers
 * Used when tasks run in user mode (PSW.IO = 0)
 */
StatusType SysActivateTask(TaskType TaskID)
{
    return osek_syscall1(SYSCALL_ACTIVATE_TASK, (uint32_t)TaskID);
}

StatusType SysTerminateTask(void)
{
    return osek_syscall0(SYSCALL_TERMINATE_TASK);
}

StatusType SysChainTask(TaskType TaskID)
{
    return osek_syscall1(SYSCALL_CHAIN_TASK, (uint32_t)TaskID);
}

StatusType SysSchedule(void)
{
    return osek_syscall0(SYSCALL_SCHEDULE);
}

StatusType SysGetResource(ResourceType ResID)
{
    return osek_syscall1(SYSCALL_GET_RESOURCE, (uint32_t)ResID);
}

StatusType SysReleaseResource(ResourceType ResID)
{
    return osek_syscall1(SYSCALL_RELEASE_RESOURCE, (uint32_t)ResID);
}

StatusType SysSetEvent(TaskType TaskID, EventMaskType Mask)
{
    return osek_syscall2(SYSCALL_SET_EVENT, (uint32_t)TaskID, (uint32_t)Mask);
}

StatusType SysClearEvent(EventMaskType Mask)
{
    return osek_syscall1(SYSCALL_CLEAR_EVENT, (uint32_t)Mask);
}

StatusType SysWaitEvent(EventMaskType Mask)
{
    return osek_syscall1(SYSCALL_WAIT_EVENT, (uint32_t)Mask);
}

StatusType SysSetRelAlarm(AlarmType AlarmID, TickType increment, TickType cycle)
{
    return osek_syscall3(SYSCALL_SET_REL_ALARM, (uint32_t)AlarmID, 
                         (uint32_t)increment, (uint32_t)cycle);
}

StatusType SysCancelAlarm(AlarmType AlarmID)
{
    return osek_syscall1(SYSCALL_CANCEL_ALARM, (uint32_t)AlarmID);
}

void SysShutdownOS(StatusType Error)
{
    osek_syscall1(SYSCALL_SHUTDOWN_OS, (uint32_t)Error);
}
