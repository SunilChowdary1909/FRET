/*
 * OSEK/RTA_OS Bindings for FRET Fuzzer
 * Target: AURIX TC4x (TriCore)
 * 
 * These structures mirror the OSEK kernel data structures in osek.h
 * for reading system state from QEMU.
 * 
 * Note: Manually written to match osek.h
 */

#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code, deref_nullptr, unused)]

use serde::{Deserialize, Serialize};
use libafl_qemu::Qemu;

/*============================================================================
 * Basic Types (matching osek.h AUTOSAR types)
 *============================================================================*/

pub type boolean = ::std::os::raw::c_uchar;
pub type uint8 = ::std::os::raw::c_uchar;
pub type uint16 = ::std::os::raw::c_ushort;
pub type uint32 = ::std::os::raw::c_uint;
pub type uint64 = ::std::os::raw::c_ulonglong;
pub type sint8 = ::std::os::raw::c_char;
pub type sint16 = ::std::os::raw::c_short;
pub type sint32 = ::std::os::raw::c_int;

pub type StatusType = uint8;
pub type AppModeType = uint32;
pub type TickType = uint32;
pub type EventMaskType = uint32;
pub type TaskStateType = uint8;
pub type CoreIdType = uint8;

/* Pointer types (32-bit addresses for TriCore) */
pub type void_ptr = ::std::os::raw::c_uint;
pub type TaskType_ptr = ::std::os::raw::c_uint;       // Pointer to Os_TaskType (const*)
pub type ResourceType_ptr = ::std::os::raw::c_uint;   // Pointer to Os_ResourceType (const*)
pub type CounterType_ptr = ::std::os::raw::c_uint;    // Pointer to Os_CounterType (const*)
pub type AlarmType_ptr = ::std::os::raw::c_uint;      // Pointer to Os_AlarmType (const*)

/*============================================================================
 * Task States (matching osek.h)
 *============================================================================*/

pub const SUSPENDED: TaskStateType = 0;
pub const READY: TaskStateType = 1;
pub const WAITING: TaskStateType = 2;
pub const RUNNING: TaskStateType = 3;

pub const INVALID_TASK: TaskType_ptr = 0;

/*============================================================================
 * Status Codes (matching osek.h)
 *============================================================================*/

pub const E_OK: StatusType = 0;
pub const E_OS_ACCESS: StatusType = 1;
pub const E_OS_CALLEVEL: StatusType = 2;
pub const E_OS_ID: StatusType = 3;
pub const E_OS_LIMIT: StatusType = 4;
pub const E_OS_NOFUNC: StatusType = 5;
pub const E_OS_RESOURCE: StatusType = 6;
pub const E_OS_STATE: StatusType = 7;
pub const E_OS_VALUE: StatusType = 8;
pub const E_OS_PARAM_POINTER: StatusType = 9;

/*============================================================================
 * Configuration Limits (matching osek.h)
 *============================================================================*/

pub const OS_MAX_TASKS: usize = 32;
pub const OS_MAX_RESOURCES: usize = 16;
pub const OS_MAX_ALARMS: usize = 16;
pub const OS_MAX_COUNTERS: usize = 4;
pub const OS_MAX_PRIORITY: usize = 64;

/*============================================================================
 * Task Static Configuration - Os_TaskType (matches osek.h)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_TaskType {
    pub index: uint8,
    pub basePriority: uint8,
    pub maxActivations: uint8,
    pub autostart: boolean,
    pub stackSize: uint32,
    pub entry: uint32,  // Function pointer as u32
}

/*============================================================================
 * Task Dynamic State - Os_TaskDynType (matches osek.h)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_TaskDynType {
    pub state: TaskStateType,
    pub currentPriority: uint8,
    pub activationCount: uint8,
    pub _pad: uint8,
    pub eventsSet: EventMaskType,
    pub eventsWaiting: EventMaskType,
    pub resourcesHeld: uint32,
}

/*============================================================================
 * Resource Static Configuration - Os_ResourceType (matches osek.h)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_ResourceType {
    pub index: uint8,
    pub ceilingPriority: uint8,
}

/*============================================================================
 * Resource Dynamic State - Os_ResourceDynType (matches osek.h)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_ResourceDynType {
    pub owner: TaskType_ptr,      // 4 bytes (pointer)
    pub prevPriority: uint8,      // 1 byte
    pub isOccupied: boolean,      // 1 byte
    pub _pad: [uint8; 2],         // 2 bytes padding
}

/*============================================================================
 * Counter Static Configuration - Os_CounterType (matches osek.h)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_CounterType {
    pub index: uint8,
    pub _pad: [uint8; 3],       // 3 bytes padding
    pub maxAllowedValue: TickType,
    pub ticksPerBase: TickType,
    pub minCycle: TickType,
}

/*============================================================================
 * Counter Dynamic State - Os_CounterDynType (matches osek.h)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_CounterDynType {
    pub value: TickType,
}

/*============================================================================
 * Alarm Action Types (matching osek.h)
 *============================================================================*/

pub const OS_ALARM_ACTION_ACTIVATETASK: uint8 = 0;
pub const OS_ALARM_ACTION_SETEVENT: uint8 = 1;
pub const OS_ALARM_ACTION_CALLBACK: uint8 = 2;

/*============================================================================
 * Alarm Static Configuration - Os_AlarmType (matches osek.h)
 * Note: Simplified - action union represented as max-size array
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_AlarmType {
    pub index: uint8,
    pub _pad1: uint8,
    pub _pad2: uint8,
    pub actionType: uint8,
    pub counter: CounterType_ptr,
    /* Action union as bytes (TaskType + EventMaskType = 8 bytes max) */
    pub actionData: [uint32; 2],
}

/*============================================================================
 * Alarm Dynamic State - Os_AlarmDynType (matches osek.h)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Os_AlarmDynType {
    pub isActive: boolean,
    pub _pad1: uint8,
    pub _pad2: uint8,
    pub _pad3: uint8,
    pub expireTime: TickType,
    pub cycle: TickType,
}

/*============================================================================
 * AlarmBase (for GetAlarmBase)
 *============================================================================*/

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct AlarmBaseType {
    pub maxallowedvalue: TickType,
    pub ticksperbase: TickType,
    pub mincycle: TickType,
}

/*============================================================================
 * Combined Task Info (for fuzzer state capture)
 * Merges static config + dynamic state for easy use
 *============================================================================*/

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskInfo {
    /* From Os_TaskType (static) */
    pub index: uint8,
    pub basePriority: uint8,
    pub maxActivations: uint8,
    pub autostart: boolean,
    pub stackSize: uint32,
    pub entry: uint32,
    /* From Os_TaskDynType (dynamic) */
    pub state: TaskStateType,
    pub currentPriority: uint8,
    pub activationCount: uint8,
    pub eventsSet: EventMaskType,
    pub eventsWaiting: EventMaskType,
    pub resourcesHeld: uint32,
    /* Task name (from application config) */
    pub taskName: String,
}

impl TaskInfo {
    pub fn from_static_and_dyn(
        static_cfg: &Os_TaskType,
        dyn_state: &Os_TaskDynType,
        name: String,
    ) -> Self {
        TaskInfo {
            index: static_cfg.index,
            basePriority: static_cfg.basePriority,
            maxActivations: static_cfg.maxActivations,
            autostart: static_cfg.autostart,
            stackSize: static_cfg.stackSize,
            entry: static_cfg.entry,
            state: dyn_state.state,
            currentPriority: dyn_state.currentPriority,
            activationCount: dyn_state.activationCount,
            eventsSet: dyn_state.eventsSet,
            eventsWaiting: dyn_state.eventsWaiting,
            resourcesHeld: dyn_state.resourcesHeld,
            taskName: name,
        }
    }
}

/*============================================================================
 * Global OS State Variables (symbols to look up in ELF)
 * 
 * In C:
 *   extern Os_TaskDynType       Os_TaskDyn[OS_MAX_TASKS];
 *   extern Os_ResourceDynType   Os_ResourceDyn[OS_MAX_RESOURCES];
 *   extern Os_AlarmDynType      Os_AlarmDyn[OS_MAX_ALARMS];
 *   extern Os_CounterDynType    Os_CounterDyn[OS_MAX_COUNTERS];
 *   extern volatile TickType    Os_TickCounter;
 *============================================================================*/

/* Symbol names for lookup */
pub const SYM_TASK_DYN: &str = "Os_TaskDyn";
pub const SYM_RESOURCE_DYN: &str = "Os_ResourceDyn";
pub const SYM_ALARM_DYN: &str = "Os_AlarmDyn";
pub const SYM_COUNTER_DYN: &str = "Os_CounterDyn";
pub const SYM_TICK_COUNTER: &str = "Os_TickCounter";

/* 
 * Additional symbols needed by fuzzer:
 * - Task static config table (application-defined)
 * - Current task pointer
 * - Ready queue structure
 */
pub const SYM_CURRENT_TASK: &str = "Os_CurrentTask";
pub const SYM_READY_QUEUE: &str = "Os_ReadyQueue";
pub const SYM_TASK_COUNT: &str = "Os_TaskCount";
