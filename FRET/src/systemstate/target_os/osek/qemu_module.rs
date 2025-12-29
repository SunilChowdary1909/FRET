/*
 * OSEK/RTA_OS QEMU Module for FRET Fuzzer
 * Hooks into QEMU execution to capture system states
 * Target: AURIX TC4x (TriCore)
 * 
 * Reads from C structures defined in osek.h:
 *   Os_TaskDyn[], Os_ResourceDyn[], Os_AlarmDyn[], Os_CounterDyn[]
 */

use std::cell::RefCell;
use std::rc::Rc;
use std::borrow::Cow;
use std::ops::Range;

use hashbrown::HashMap;
use itertools::Itertools;

use libafl::{
    inputs::UsesInput,
    prelude::{ExitKind, ObserversTuple},
    HasMetadata,
};
use libafl_qemu::{
    modules::{EmulatorModule, EmulatorModuleTuple, NopAddressFilter, NopPageFilter},
    sys::TCGTemp,
    EmulatorModules, GuestAddr, Hook, MemAccessInfo,
};

use crate::{
    fuzzer::MAX_INPUT_SIZE,
    systemstate::{
        helpers::{get_icount, in_any_range, read_rec_return_stackframe},
        target_os::{osek::bindings::*, compute_hash, QemuLookup},
        AtomicBasicBlock, CaptureEvent, ExecInterval, RTOSJob,
    },
};

use super::{
    OSEKSystemState, OSEKSystemStateContext, OSEKTraceMetadata,
    RawOSEKSystemState, RefinedTCB, CURRENT_SYSTEMSTATE_VEC, JOBS_DONE,
    ISR_SYMBOLS, USR_ISR_SYMBOLS,
};

/*============================================================================
 * QEMU Helper Structure
 *============================================================================*/

/// QEMU Helper that reads OSEK specific structs from QEMU
#[derive(Debug)]
pub struct OSEKSystemStateHelper {
    // Address ranges
    pub app_range: Range<GuestAddr>,
    
    // API function addresses
    pub api_fn_addrs: HashMap<GuestAddr, Cow<'static, str>>,
    pub api_fn_ranges: Vec<(Cow<'static, str>, Range<GuestAddr>)>,
    
    // ISR addresses
    pub isr_fn_addrs: HashMap<GuestAddr, Cow<'static, str>>,
    pub isr_fn_ranges: Vec<(Cow<'static, str>, Range<GuestAddr>)>,
    
    // Input memory range
    pub input_mem: Range<GuestAddr>,
    
    // OSEK symbol addresses (matching osek.h globals)
    pub task_dyn_addr: GuestAddr,       // Os_TaskDyn[]
    pub task_count_addr: GuestAddr,     // Os_TaskCount
    pub task_cfg_addr: GuestAddr,       // Os_TaskCfg[] (static configs)
    pub current_task_addr: GuestAddr,   // Os_CurrentTask
    pub resource_dyn_addr: GuestAddr,   // Os_ResourceDyn[]
    pub resource_count_addr: GuestAddr, // Os_ResourceCount
    pub alarm_dyn_addr: GuestAddr,      // Os_AlarmDyn[]
    pub alarm_count_addr: GuestAddr,    // Os_AlarmCount
    pub counter_dyn_addr: GuestAddr,    // Os_CounterDyn[]
    pub counter_count_addr: GuestAddr,  // Os_CounterCount
    pub tick_counter_addr: GuestAddr,   // Os_TickCounter
    pub job_done_addr: GuestAddr,       // trigger_job_done
}

impl OSEKSystemStateHelper {
    #[must_use]
    pub fn new(
        target_symbols: &HashMap<&'static str, GuestAddr>,
        target_ranges: &HashMap<&'static str, Range<GuestAddr>>,
        target_groups: &HashMap<&'static str, HashMap<String, Range<GuestAddr>>>,
    ) -> Self {
        let app_range = target_ranges.get("APP_CODE").unwrap().clone();

        let api_fn_ranges: Vec<_> = target_groups
            .get("API_FN")
            .unwrap()
            .iter()
            .sorted_by_key(|x| x.1.start)
            .map(|(n, r)| {
                (
                    Cow::Borrowed(Box::leak(n.clone().into_boxed_str()) as &'static str),
                    r.clone(),
                )
            })
            .collect();
        let api_fn_addrs = api_fn_ranges
            .iter()
            .map(|(n, r)| (r.start, n.clone()))
            .collect();

        let isr_fn_ranges: Vec<_> = target_groups
            .get("ISR_FN")
            .unwrap()
            .iter()
            .sorted_by_key(|x| x.1.start)
            .map(|(n, r)| {
                (
                    Cow::Borrowed(Box::leak(n.clone().into_boxed_str()) as &'static str),
                    r.clone(),
                )
            })
            .collect();
        let isr_fn_addrs = isr_fn_ranges
            .iter()
            .map(|(n, r)| (r.start, n.clone()))
            .collect();

        let input_mem = target_symbols
            .get("FUZZ_INPUT")
            .map(|x| *x..(*x + unsafe { MAX_INPUT_SIZE as GuestAddr }))
            .unwrap();

        OSEKSystemStateHelper {
            app_range,
            api_fn_addrs,
            api_fn_ranges,
            isr_fn_addrs,
            isr_fn_ranges,
            input_mem,
            task_dyn_addr: *target_symbols.get("Os_TaskDyn").unwrap_or(&0),
            task_count_addr: *target_symbols.get("Os_TaskCount").unwrap_or(&0),
            task_cfg_addr: *target_symbols.get("Os_TaskCfg").unwrap_or(&0),
            current_task_addr: *target_symbols.get("Os_CurrentTask").unwrap_or(&0),
            resource_dyn_addr: *target_symbols.get("Os_ResourceDyn").unwrap_or(&0),
            resource_count_addr: *target_symbols.get("Os_ResourceCount").unwrap_or(&0),
            alarm_dyn_addr: *target_symbols.get("Os_AlarmDyn").unwrap_or(&0),
            alarm_count_addr: *target_symbols.get("Os_AlarmCount").unwrap_or(&0),
            counter_dyn_addr: *target_symbols.get("Os_CounterDyn").unwrap_or(&0),
            counter_count_addr: *target_symbols.get("Os_CounterCount").unwrap_or(&0),
            tick_counter_addr: *target_symbols.get("Os_TickCounter").unwrap_or(&0),
            job_done_addr: *target_symbols.get("trigger_job_done").unwrap_or(&0),
        }
    }
}

/*============================================================================
 * Global State for Hooks
 *============================================================================*/

static mut INPUT_MEM: Range<GuestAddr> = 0..0;
pub static mut MEM_READ: Vec<(u32, u8)> = Vec::new();
static mut JOBS_DONE: Vec<(String, u64, u64)> = Vec::new();

/*============================================================================
 * System State Capture
 *============================================================================*/

/// Read a u32 from QEMU memory
fn read_u32(emulator: &libafl_qemu::Qemu, addr: GuestAddr) -> u32 {
    let mut bytes = [0u8; 4];
    unsafe {
        let _ = emulator.read_mem(addr.into(), &mut bytes);
    }
    u32::from_le_bytes(bytes)
}

/// Read the current OSEK system state from QEMU
fn capture_osek_state(
    emulator: &libafl_qemu::Qemu,
    helper: &OSEKSystemStateHelper,
    event: CaptureEvent,
    pc: GuestAddr,
) -> RawOSEKSystemState {
    let icount = get_icount(emulator);
    
    // Read task count
    let task_count = if helper.task_count_addr != 0 {
        read_u32(emulator, helper.task_count_addr) as usize
    } else {
        0
    };
    let task_count = task_count.min(OS_MAX_TASKS);
    
    // Read tick counter
    let tick_count = if helper.tick_counter_addr != 0 {
        read_u32(emulator, helper.tick_counter_addr)
    } else {
        0
    };
    
    // Read current task index
    let current_task_idx = if helper.current_task_addr != 0 {
        read_u32(emulator, helper.current_task_addr) as u8
    } else {
        0xFF
    };
    
    // Read task dynamic states
    let mut task_dyn_states = Vec::with_capacity(task_count);
    if helper.task_dyn_addr != 0 {
        let dyn_size = std::mem::size_of::<Os_TaskDynType>() as GuestAddr;
        for i in 0..task_count {
            let addr = helper.task_dyn_addr + (i as GuestAddr * dyn_size);
            let dyn_state: Os_TaskDynType = QemuLookup::lookup(emulator, addr);
            task_dyn_states.push(dyn_state);
        }
    }
    
    // Read task static configs (if available)
    let mut task_configs = Vec::with_capacity(task_count);
    if helper.task_cfg_addr != 0 {
        let cfg_size = std::mem::size_of::<Os_TaskType>() as GuestAddr;
        for i in 0..task_count {
            let addr = helper.task_cfg_addr + (i as GuestAddr * cfg_size);
            let cfg: Os_TaskType = QemuLookup::lookup(emulator, addr);
            task_configs.push(cfg);
        }
    } else {
        // Create dummy configs
        for i in 0..task_count {
            task_configs.push(Os_TaskType {
                index: i as u8,
                basePriority: 0,
                maxActivations: 1,
                autostart: 0,
                stackSize: 0,
                entry: 0,
            });
        }
    }
    
    // Task names (would need to be read from application config)
    let task_names: Vec<String> = (0..task_count)
        .map(|i| format!("Task{}", i))
        .collect();
    
    // Read resource states
    let resource_count = if helper.resource_count_addr != 0 {
        (read_u32(emulator, helper.resource_count_addr) as usize).min(OS_MAX_RESOURCES)
    } else {
        0
    };
    
    let mut resource_dyn_states = Vec::with_capacity(resource_count);
    if helper.resource_dyn_addr != 0 {
        let dyn_size = std::mem::size_of::<Os_ResourceDynType>() as GuestAddr;
        for i in 0..resource_count {
            let addr = helper.resource_dyn_addr + (i as GuestAddr * dyn_size);
            let dyn_state: Os_ResourceDynType = QemuLookup::lookup(emulator, addr);
            resource_dyn_states.push(dyn_state);
        }
    }
    
    // Read alarm states
    let alarm_count = if helper.alarm_count_addr != 0 {
        (read_u32(emulator, helper.alarm_count_addr) as usize).min(OS_MAX_ALARMS)
    } else {
        0
    };
    
    let mut alarm_dyn_states = Vec::with_capacity(alarm_count);
    if helper.alarm_dyn_addr != 0 {
        let dyn_size = std::mem::size_of::<Os_AlarmDynType>() as GuestAddr;
        for i in 0..alarm_count {
            let addr = helper.alarm_dyn_addr + (i as GuestAddr * dyn_size);
            let dyn_state: Os_AlarmDynType = QemuLookup::lookup(emulator, addr);
            alarm_dyn_states.push(dyn_state);
        }
    }
    
    // Read counter states
    let counter_count = if helper.counter_count_addr != 0 {
        (read_u32(emulator, helper.counter_count_addr) as usize).min(OS_MAX_COUNTERS)
    } else {
        0
    };
    
    let mut counter_dyn_states = Vec::with_capacity(counter_count);
    if helper.counter_dyn_addr != 0 {
        let dyn_size = std::mem::size_of::<Os_CounterDynType>() as GuestAddr;
        for i in 0..counter_count {
            let addr = helper.counter_dyn_addr + (i as GuestAddr * dyn_size);
            let dyn_state: Os_CounterDynType = QemuLookup::lookup(emulator, addr);
            counter_dyn_states.push(dyn_state);
        }
    }
    
    RawOSEKSystemState {
        current_task_idx,
        task_configs,
        task_dyn_states,
        task_names,
        resource_dyn_states,
        alarm_dyn_states,
        counter_dyn_states,
        tick_count,
        icount,
        event,
        pc,
    }
}

/// Trigger system state collection
pub fn trigger_collection(
    emulator: &libafl_qemu::Qemu,
    helper: &OSEKSystemStateHelper,
    event: CaptureEvent,
    pc: GuestAddr,
) {
    let state = capture_osek_state(emulator, helper, event, pc);
    unsafe {
        CURRENT_SYSTEMSTATE_VEC.push(state);
    }
}

/*============================================================================
 * QEMU Hooks
 *============================================================================*/

/// Hook called on ISR entry
fn exec_isr_hook<ET, S>(
    _emulator_modules: &mut EmulatorModules<ET, S>,
    _state: Option<&mut S>,
    pc: GuestAddr,
) where
    ET: EmulatorModuleTuple<S>,
    S: UsesInput + Unpin + HasMetadata,
{
    // Capture state on ISR entry
    // Implementation would capture the state here
}

/// Hook for jump instructions (syscalls, etc.)
fn gen_jmp_is_syscall<ET, S>(
    _emulator_modules: &mut EmulatorModules<ET, S>,
    _state: Option<&mut S>,
    _src: Option<GuestAddr>,
    _dest: GuestAddr,
) -> Option<u64>
where
    ET: EmulatorModuleTuple<S>,
    S: UsesInput + Unpin + HasMetadata,
{
    // Check if this is a syscall/API entry
    None
}

/// Trace jump execution
fn trace_jmp<ET, S>(
    _emulator_modules: &mut EmulatorModules<ET, S>,
    _state: Option<&mut S>,
    _id: u64,
    _src: GuestAddr,
    _dest: GuestAddr,
) where
    ET: EmulatorModuleTuple<S>,
    S: UsesInput + Unpin + HasMetadata,
{
    // Trace jump for coverage
}

/// Hook for job completion
fn job_done_hook<ET, S>(
    emulator_modules: &mut EmulatorModules<ET, S>,
    _state: Option<&mut S>,
    _pc: GuestAddr,
) where
    ET: EmulatorModuleTuple<S>,
    S: UsesInput + Unpin + HasMetadata,
{
    // Record job completion for timing analysis
}

/// Check if read is from input memory
fn gen_read_is_input<ET, S>(
    _emulator_modules: &mut EmulatorModules<ET, S>,
    _state: Option<&mut S>,
    _pc: GuestAddr,
    addr: *mut TCGTemp,
    _info: MemAccessInfo,
) -> Option<u64>
where
    ET: EmulatorModuleTuple<S>,
    S: UsesInput + Unpin + HasMetadata,
{
    None
}

/// Trace memory reads
fn trace_reads<ET, S>(
    _emulator_modules: &mut EmulatorModules<ET, S>,
    _state: Option<&mut S>,
    _id: u64,
    _pc: GuestAddr,
    addr: GuestAddr,
    size: usize,
) where
    ET: EmulatorModuleTuple<S>,
    S: UsesInput + Unpin + HasMetadata,
{
    // Record input memory reads
}

/*============================================================================
 * EmulatorModule Implementation
 *============================================================================*/

impl<S, I> EmulatorModule<S> for OSEKSystemStateHelper
where
    S: UsesInput<Input = I> + Unpin + HasMetadata,
{
    fn first_exec<ET>(&mut self, emulator_modules: &mut EmulatorModules<ET, S>, _state: &mut S)
    where
        ET: EmulatorModuleTuple<S>,
    {
        // Install hooks for ISR entry
        for wp in self.isr_fn_addrs.keys() {
            emulator_modules.instructions(*wp, Hook::Function(exec_isr_hook::<ET, S>), false);
        }
        
        // Install jump hooks for syscall detection
        emulator_modules.jmps(
            Hook::Function(gen_jmp_is_syscall::<ET, S>),
            Hook::Function(trace_jmp::<ET, S>),
        );
        
        // Job completion hook
        #[cfg(feature = "trace_job_response_times")]
        emulator_modules.instructions(
            self.job_done_addr,
            Hook::Function(job_done_hook::<ET, S>),
            false,
        );
        
        // Memory read hooks
        #[cfg(feature = "trace_reads")]
        emulator_modules.reads(
            Hook::Function(gen_read_is_input::<ET, S>),
            Hook::Empty,
            Hook::Empty,
            Hook::Empty,
            Hook::Empty,
            Hook::Function(trace_reads::<ET, S>),
        );
        
        unsafe {
            INPUT_MEM = self.input_mem.clone();
        }
    }

    fn pre_exec<ET>(
        &mut self,
        _emulator_modules: &mut EmulatorModules<ET, S>,
        state: &mut S,
        _input: &S::Input,
    ) where
        ET: EmulatorModuleTuple<S>,
    {
        unsafe {
            CURRENT_SYSTEMSTATE_VEC.clear();
            JOBS_DONE.clear();
        }
        
        if state.has_metadata::<OSEKTraceMetadata>() {
            state.remove_metadata::<OSEKTraceMetadata>();
        }
    }

    fn post_exec<ET, OT>(
        &mut self,
        emulator_modules: &mut EmulatorModules<ET, S>,
        state: &mut S,
        _input: &S::Input,
        _observers: &mut OT,
        exit_kind: &mut ExitKind,
    ) where
        ET: EmulatorModuleTuple<S>,
        OT: ObserversTuple<S::Input, S>,
    {
        let mut need_to_debug = false;
        if unsafe { CURRENT_SYSTEMSTATE_VEC.len() } == 0 {
            eprintln!("No system states captured, aborting");
            return;
        }
        
        // Collect final state
        let c = emulator_modules.qemu().cpu_from_index(0);
        let pc = c.read_reg::<_, u32>(libafl_qemu::regs::Regs::Pc).unwrap() as GuestAddr;
        trigger_collection(&emulator_modules.qemu(), self, CaptureEvent::End, pc);
        
        // Process captured states
        let raw_states = unsafe { CURRENT_SYSTEMSTATE_VEC.split_off(0) };
        let mut refined_states = Vec::new();
        
        for raw in &raw_states {
            refined_states.push(OSEKSystemState::from_raw(raw));
        }
        
        // Build execution intervals from state transitions
        let mut intervals = Vec::new();
        for i in 0..raw_states.len().saturating_sub(1) {
            let start = &raw_states[i];
            let end = &raw_states[i + 1];
            let start_state = &refined_states[i];
            
            let task_name = if start.current_task_idx != 0xFF && (start.current_task_idx as usize) < start.task_names.len() {
                Cow::Owned(start.task_names[start.current_task_idx as usize].clone())
            } else {
                Cow::Borrowed("IDLE")
            };
            
            let interval = ExecInterval {
                start_tick: start.icount,
                end_tick: end.icount,
                start_state_hash: compute_hash(start_state),
                end_state_hash: compute_hash(&refined_states[i + 1]),
                task_name,
                abb: AtomicBasicBlock::default(),
            };
            intervals.push(interval);
        }
        
        // Build job records
        #[cfg(not(feature = "trace_job_response_times"))]
        let jobs = Vec::new();
        #[cfg(feature = "trace_job_response_times")]
        let jobs = unsafe {
            JOBS_DONE.iter().map(|(name, release, response)| RTOSJob {
                name: name.clone(),
                release: *release,
                response: *response,
                exec_ticks: response - release,
                preemptions: 0,
            }).collect()
        };
        
        let metadata = OSEKTraceMetadata::new(
            refined_states,
            intervals,
            vec![unsafe { MEM_READ.clone() }],
            jobs,
            need_to_debug,
        );
        
        state.add_metadata(metadata);
        
        unsafe {
            MEM_READ.clear();
            JOBS_DONE.clear();
        }
    }
}
