/*
 * OSEK/RTA_OS System State Module for FRET Fuzzer
 * Main module defining system state structures and traits
 * Target: AURIX TC4x (TriCore)
 */

use libafl_qemu::GuestAddr;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use hashbrown::HashMap;

use crate::{
    impl_emu_lookup,
    systemstate::{helpers::get_icount, CaptureEvent},
};

pub mod bindings;
pub mod config;
pub mod qemu_module;

use bindings::*;

use super::QemuLookup;
use crate::systemstate::target_os::*;
use crate::systemstate::{ExecInterval, RTOSJob, AtomicBasicBlock};

/*============================================================================
 * Constants
 *============================================================================*/

/// ISR symbols for OSEK/TriCore
pub const ISR_SYMBOLS: &'static [&'static str] = &[
    // TriCore trap handlers
    "Os_TrapHandler_MMU",
    "Os_TrapHandler_Protection",
    "Os_TrapHandler_Instruction",
    "Os_TrapHandler_Context",
    "Os_TrapHandler_Bus",
    "Os_TrapHandler_Assertion",
    "Os_TrapHandler_Syscall",
    "Os_TrapHandler_NMI",
    // OS tick handler
    "Os_TickHandler",
    // Context switch handler
    "Os_ContextSwitchHandler",
    // User ISR handlers (Category 2)
    "ISR_0_Handler",
    "ISR_1_Handler",
    "ISR_2_Handler",
    "ISR_3_Handler",
    "ISR_4_Handler",
    "ISR_5_Handler",
    "ISR_6_Handler",
    "ISR_7_Handler",
];

pub const USR_ISR_SYMBOLS: &'static [&'static str] = &[
    "ISR_0_Handler",
    "ISR_1_Handler",
    "ISR_2_Handler",
    "ISR_3_Handler",
    "ISR_4_Handler",
    "ISR_5_Handler",
    "ISR_6_Handler",
    "ISR_7_Handler",
];

/*============================================================================
 * System Type Implementation
 *============================================================================*/

/// Main OSEK system type implementing TargetSystem trait
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OSEKSystem {
    pub raw_trace: Vec<RawOSEKSystemState>,
}

impl TargetSystem for OSEKSystem {
    type State = OSEKSystemState;
    type TCB = RefinedTCB;
    type TraceData = OSEKTraceMetadata;
}

/*============================================================================
 * Task Control Block (Refined) - for fuzzer use
 *============================================================================*/

#[derive(Debug, Clone, Default, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct RefinedTCB {
    pub task_index: uint8,
    pub task_name: String,
    pub state: TaskStateType,
    pub base_priority: uint8,
    pub current_priority: uint8,
    pub activation_count: uint8,
    pub max_activations: uint8,
    pub events_waiting: EventMaskType,
    pub events_set: EventMaskType,
    pub resources_held: uint32,
}

impl TaskControlBlock for RefinedTCB {
    fn task_name(&self) -> &String {
        &self.task_name
    }
    fn task_name_mut(&mut self) -> &mut String {
        &mut self.task_name
    }
}

impl RefinedTCB {
    /// Create from static config + dynamic state
    pub fn from_static_and_dyn(
        static_cfg: &Os_TaskType,
        dyn_state: &Os_TaskDynType,
        name: String,
    ) -> Self {
        RefinedTCB {
            task_index: static_cfg.index,
            task_name: name,
            state: dyn_state.state,
            base_priority: static_cfg.basePriority,
            current_priority: dyn_state.currentPriority,
            activation_count: dyn_state.activationCount,
            max_activations: static_cfg.maxActivations,
            events_waiting: dyn_state.eventsWaiting,
            events_set: dyn_state.eventsSet,
            resources_held: dyn_state.resourcesHeld,
        }
    }
}

/*============================================================================
 * Raw System State (captured from QEMU)
 *============================================================================*/

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawOSEKSystemState {
    /// Current task index (0xFF = no task)
    pub current_task_idx: uint8,
    /// All task static configs
    pub task_configs: Vec<Os_TaskType>,
    /// All task dynamic states
    pub task_dyn_states: Vec<Os_TaskDynType>,
    /// Task names (from application)
    pub task_names: Vec<String>,
    /// Resource dynamic states
    pub resource_dyn_states: Vec<Os_ResourceDynType>,
    /// Alarm dynamic states
    pub alarm_dyn_states: Vec<Os_AlarmDynType>,
    /// Counter dynamic states
    pub counter_dyn_states: Vec<Os_CounterDynType>,
    /// Tick counter at capture time
    pub tick_count: TickType,
    /// Instruction count at capture time
    pub icount: u64,
    /// Capture event type
    pub event: CaptureEvent,
    /// PC at capture
    pub pc: GuestAddr,
}

/*============================================================================
 * Refined System State
 *============================================================================*/

#[derive(Debug, Clone, Default, Serialize, Deserialize, Hash, PartialEq)]
pub struct OSEKSystemState {
    pub current_task: RefinedTCB,
    pub ready_list: Vec<RefinedTCB>,
    pub waiting_list: Vec<RefinedTCB>,
    pub suspended_list: Vec<RefinedTCB>,
    pub tick_count: TickType,
    pub scheduler_locked: bool,
}

impl SystemState for OSEKSystemState {
    type TCB = RefinedTCB;

    fn current_task(&self) -> &Self::TCB {
        &self.current_task
    }

    fn current_task_mut(&mut self) -> &mut Self::TCB {
        &mut self.current_task
    }

    fn get_ready_lists(&self) -> &Vec<Self::TCB> {
        &self.ready_list
    }

    fn get_delay_list(&self) -> &Vec<Self::TCB> {
        &self.waiting_list
    }

    fn print_lists(&self) -> String {
        let mut result = String::new();
        result.push_str(&format!("Current: {}\n", self.current_task.task_name));
        result.push_str("Ready: ");
        for tcb in &self.ready_list {
            result.push_str(&format!("{} ", tcb.task_name));
        }
        result.push_str("\nWaiting: ");
        for tcb in &self.waiting_list {
            result.push_str(&format!("{} ", tcb.task_name));
        }
        result
    }
}

impl OSEKSystemState {
    pub fn from_raw(raw: &RawOSEKSystemState) -> Self {
        let mut current_task = RefinedTCB::default();
        let mut ready_list = Vec::new();
        let mut waiting_list = Vec::new();
        let mut suspended_list = Vec::new();

        // Combine static config + dynamic state for each task
        for i in 0..raw.task_configs.len() {
            let static_cfg = &raw.task_configs[i];
            let dyn_state = &raw.task_dyn_states[i];
            let name = raw.task_names.get(i)
                .cloned()
                .unwrap_or_else(|| format!("Task{}", i));

            let refined = RefinedTCB::from_static_and_dyn(static_cfg, dyn_state, name);

            match dyn_state.state {
                RUNNING => current_task = refined,
                READY => ready_list.push(refined),
                WAITING => waiting_list.push(refined),
                SUSPENDED => suspended_list.push(refined),
                _ => suspended_list.push(refined),
            }
        }

        // Sort ready list by priority (highest first)
        ready_list.sort_by(|a, b| b.current_priority.cmp(&a.current_priority));

        OSEKSystemState {
            current_task,
            ready_list,
            waiting_list,
            suspended_list,
            tick_count: raw.tick_count,
            scheduler_locked: false,
        }
    }
}

/*============================================================================
 * Trace Metadata
 *============================================================================*/

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OSEKTraceMetadata {
    /// Reference count for LibAFL
    ref_cnt: usize,
    /// Map of state hash to state
    states_map: HashMap<u64, OSEKSystemState>,
    /// Execution intervals
    intervals: Vec<ExecInterval>,
    /// Memory reads during execution
    mem_reads: Vec<Vec<(u32, u8)>>,
    /// RTOS jobs executed
    jobs: Vec<RTOSJob>,
    /// Debug flag
    need_debug: bool,
}

impl OSEKTraceMetadata {
    pub fn new(
        trace: Vec<<OSEKTraceMetadata as SystemTraceData>::State>,
        intervals: Vec<ExecInterval>,
        mem_reads: Vec<Vec<(u32, u8)>>,
        jobs: Vec<RTOSJob>,
        need_to_debug: bool,
    ) -> Self {
        let mut states_map = HashMap::new();
        for state in trace {
            let hash = compute_hash(&state);
            states_map.insert(hash, state);
        }
        OSEKTraceMetadata {
            ref_cnt: 1,
            states_map,
            intervals,
            mem_reads,
            jobs,
            need_debug: need_to_debug,
        }
    }
}

impl libafl_bolts::HasRefCnt for OSEKTraceMetadata {
    fn refcnt(&self) -> isize {
        self.ref_cnt as isize
    }
    fn refcnt_mut(&mut self) -> &mut isize {
        unsafe { &mut *(&mut self.ref_cnt as *mut usize as *mut isize) }
    }
}

impl SystemTraceData for OSEKTraceMetadata {
    type State = OSEKSystemState;

    fn states(&self) -> Vec<&Self::State> {
        self.states_map.values().collect()
    }

    fn states_map(&self) -> &HashMap<u64, Self::State> {
        &self.states_map
    }

    fn states_map_mut(&mut self) -> &mut HashMap<u64, Self::State> {
        &mut self.states_map
    }

    fn intervals(&self) -> &Vec<ExecInterval> {
        &self.intervals
    }

    fn intervals_mut(&mut self) -> &mut Vec<ExecInterval> {
        &mut self.intervals
    }

    fn mem_reads(&self) -> &Vec<Vec<(u32, u8)>> {
        &self.mem_reads
    }

    fn jobs(&self) -> &Vec<RTOSJob> {
        &self.jobs
    }

    fn trace_length(&self) -> usize {
        self.intervals.len()
    }

    fn need_to_debug(&self) -> bool {
        self.need_debug
    }
}

/*============================================================================
 * QEMU Memory Lookups
 *============================================================================*/

impl_emu_lookup!(Os_TaskType);
impl_emu_lookup!(Os_TaskDynType);
impl_emu_lookup!(Os_ResourceType);
impl_emu_lookup!(Os_ResourceDynType);
impl_emu_lookup!(Os_CounterType);
impl_emu_lookup!(Os_CounterDynType);
impl_emu_lookup!(Os_AlarmType);
impl_emu_lookup!(Os_AlarmDynType);
impl_emu_lookup!(void_ptr);
impl_emu_lookup!(TickType);

/*============================================================================
 * System State Context (for capture during execution)
 *============================================================================*/

#[derive(Debug, Clone, Default)]
pub struct OSEKSystemStateContext {
    pub current_state: Option<RawOSEKSystemState>,
    pub last_icount: u64,
    pub last_pc: GuestAddr,
}

/*============================================================================
 * Global State Storage
 *============================================================================*/

/// Thread-local storage for captured system states during fuzzing
pub static mut CURRENT_SYSTEMSTATE_VEC: Vec<RawOSEKSystemState> = Vec::new();
