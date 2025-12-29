use std::cell::RefCell;
use std::rc::Rc;
use std::{borrow::Cow, collections::VecDeque};
use std::ops::Range;

use freertos::{FreeRTOSTraceMetadata, USR_ISR_SYMBOLS};
use hashbrown::HashMap;

use libafl::{
    inputs::UsesInput,
    prelude::{ExitKind, ObserversTuple}, HasMetadata,
};
use libafl_qemu::{
    modules::{EmulatorModule, EmulatorModuleTuple, NopAddressFilter, NopPageFilter},
    sys::TCGTemp,
    EmulatorModules, GuestAddr, Hook, MemAccessInfo,
};

use crate::{fuzzer::MAX_INPUT_SIZE, systemstate::{
    helpers::{get_icount, in_any_range, read_rec_return_stackframe},
    target_os::{freertos::FreeRTOSStruct::*, *},
    AtomicBasicBlock, CaptureEvent, RTOSJob,
}};

use super::{
    bindings::{self, *},
    compute_hash, trigger_collection, ExecInterval, FreeRTOSStruct, FreeRTOSSystemState,
    FreeRTOSSystemStateContext, RawFreeRTOSSystemState, RefinedTCB, CURRENT_SYSTEMSTATE_VEC,
};

//============================= Qemu Helper

/// A Qemu Helper with reads FreeRTOS specific structs from Qemu whenever certain syscalls occur, also inject inputs
#[derive(Debug)]
pub struct FreeRTOSSystemStateHelper {
    // Address of the application code
    pub app_range: Range<GuestAddr>,
    // Address of API functions
    pub api_fn_addrs: HashMap<GuestAddr, Cow<'static, str>>,
    pub api_fn_ranges: Vec<(Cow<'static, str>, std::ops::Range<GuestAddr>)>,
    // Address of interrupt routines
    pub isr_fn_addrs: HashMap<GuestAddr, Cow<'static, str>>,
    pub isr_fn_ranges: Vec<(Cow<'static, str>, std::ops::Range<GuestAddr>)>,
    // Address of input memory
    pub input_mem: Range<GuestAddr>,
    // FreeRTOS specific addresses
    pub tcb_addr: GuestAddr,
    pub ready_queues: GuestAddr,
    pub delay_queue: GuestAddr,
    pub delay_queue_overflow: GuestAddr,
    pub scheduler_lock_addr: GuestAddr,
    pub scheduler_running_addr: GuestAddr,
    pub critical_addr: GuestAddr,
    pub job_done_addrs: GuestAddr,
}

impl FreeRTOSSystemStateHelper {
    #[must_use]
    pub fn new(
        target_symbols: &HashMap<&'static str, GuestAddr>,
        target_ranges: &HashMap<&'static str, Range<GuestAddr>>,
        target_groups: &HashMap<&'static str, HashMap<String, Range<GuestAddr>>>,
    ) -> Self {
        let app_range = target_ranges.get("APP_CODE").unwrap().clone();

        let api_fn_ranges : Vec<_> = target_groups.get("API_FN").unwrap().iter().sorted_by_key(|x|x.1.start).map(|(n,r)| (Cow::Borrowed(Box::leak(n.clone().into_boxed_str())),r.clone())).collect();
        let api_fn_addrs = api_fn_ranges.iter().map(|(n,r)| (r.start,n.clone())).collect();
        let isr_fn_ranges : Vec<_> = target_groups.get("ISR_FN").unwrap().iter().sorted_by_key(|x|x.1.start).map(|(n,r)| (Cow::Borrowed(Box::leak(n.clone().into_boxed_str())),r.clone())).collect();
        let isr_fn_addrs = isr_fn_ranges.iter().map(|(n,r)| (r.start,n.clone())).collect();

        let input_mem = target_symbols.get("FUZZ_INPUT").map(|x| *x..(*x+unsafe{MAX_INPUT_SIZE as GuestAddr})).unwrap();

        let tcb_addr = *target_symbols.get("pxCurrentTCB").unwrap();
        let ready_queues = *target_symbols.get("pxReadyTasksLists").unwrap();
        let delay_queue = *target_symbols.get("pxDelayedTaskList").unwrap();
        let delay_queue_overflow = *target_symbols.get("pxOverflowDelayedTaskList").unwrap();
        let scheduler_lock_addr = *target_symbols.get("uxSchedulerSuspended").unwrap();
        let scheduler_running_addr = *target_symbols.get("xSchedulerRunning").unwrap();
        let critical_addr = *target_symbols.get("uxCriticalNesting").unwrap();
        let job_done_addrs = *target_symbols.get("trigger_job_done").unwrap();

        FreeRTOSSystemStateHelper {
            app_range,
            api_fn_addrs,
            api_fn_ranges,
            isr_fn_addrs,
            isr_fn_ranges,
            input_mem,
            tcb_addr,
            ready_queues,
            delay_queue,
            delay_queue_overflow,
            scheduler_lock_addr,
            scheduler_running_addr,
            critical_addr,
            job_done_addrs,
        }
    }
}

impl<S, I> EmulatorModule<S> for FreeRTOSSystemStateHelper
where
    S: UsesInput<Input = I> + Unpin + HasMetadata,
{
    fn first_exec<ET>(&mut self, emulator_modules: &mut EmulatorModules<ET, S>, _state: &mut S)
    where
        ET: EmulatorModuleTuple<S>,
    {
        for wp in self.isr_fn_addrs.keys() {
            emulator_modules.instructions(*wp, Hook::Function(exec_isr_hook::<ET, S>), false);
        }
        emulator_modules.jmps(
            Hook::Function(gen_jmp_is_syscall::<ET, S>),
            Hook::Function(trace_jmp::<ET, S>),
        );
        #[cfg(feature = "trace_job_response_times")]
        emulator_modules.instructions(
            self.job_done_addrs,
            Hook::Function(job_done_hook::<ET, S>),
            false,
        );
        #[cfg(feature = "trace_reads")]
        emulator_modules.reads(
            Hook::Function(gen_read_is_input::<ET, S>),
            Hook::Empty,
            Hook::Empty,
            Hook::Empty,
            Hook::Empty,
            Hook::Function(trace_reads::<ET, S>),
        );
        unsafe { INPUT_MEM = self.input_mem.clone() };
    }

    // TODO: refactor duplicate code
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
        if state.has_metadata::<FreeRTOSTraceMetadata>() {
            state.remove_metadata::<FreeRTOSTraceMetadata>();
        }
    }

    fn post_exec<OT, ET>(
        &mut self,
        emulator_modules: &mut EmulatorModules<ET, S>,
        _state: &mut S,
        _input: &S::Input,
        _observers: &mut OT,
        _exit_kind: &mut ExitKind,
    ) where
        OT: ObserversTuple<S::Input, S>,
        ET: EmulatorModuleTuple<S>,
    {
        let mut need_to_debug = false;
        if unsafe { CURRENT_SYSTEMSTATE_VEC.len() } == 0 {
            eprintln!("No system states captured, aborting");
            return;
        }
        // Collect the final system state
        trigger_collection(&emulator_modules.qemu(), (0, 0), CaptureEvent::End, self);
        let c = emulator_modules.qemu().cpu_from_index(0);
        let pc = c.read_reg::<i32>(15).unwrap();
        let last = unsafe { CURRENT_SYSTEMSTATE_VEC.last_mut().unwrap() };
        last.edge = (pc, 0);
        last.capture_point =(CaptureEvent::End, Cow::Borrowed("Breakpoint"));
        // Find the first ISREnd of vPortSVCHandler (start of the first task) and drop anything before
        unsafe {
            let mut index = 0;
            while index < CURRENT_SYSTEMSTATE_VEC.len() {
                if CaptureEvent::ISREnd == CURRENT_SYSTEMSTATE_VEC[index].capture_point.0
                    && CURRENT_SYSTEMSTATE_VEC[index].capture_point.1 == "xPortPendSVHandler"
                {
                    break;
                }
                index += 1;
            }
            drop(CURRENT_SYSTEMSTATE_VEC.drain(..index));
            if CURRENT_SYSTEMSTATE_VEC.len() == 1 {
                eprintln!("No system states captured, aborting");
                return;
            }
        }
        // Start refining the state trace
        let (refined_states, metadata) =
            refine_system_states(unsafe { CURRENT_SYSTEMSTATE_VEC.split_off(0) });
        let (intervals, mem_reads, dumped_states, success) =
            states2intervals(refined_states.clone(), metadata);
        need_to_debug |= !success;
        #[cfg(not(feature = "trace_job_response_times"))]
        let jobs = Vec::new();
        #[cfg(feature = "trace_job_response_times")]
        let jobs = {
            let releases = get_releases(&intervals, &dumped_states);
            let responses = unsafe { JOBS_DONE.split_off(0) };
            let (job_spans, do_report) = get_release_response_pairs(&releases, &responses);
            need_to_debug |= do_report;

            let jobs : Vec<RTOSJob> = job_spans
                .into_iter()
                .map(|x| {
                    let intervals_of_job_x = intervals
                        .iter()
                        .enumerate()
                        .filter(|y| {
                            y.1.start_tick <= x.1
                                && y.1.end_tick >= x.0
                                && x.2 == y.1.get_task_name_unchecked()
                        })
                        .map(|(idx, x)| (x, &mem_reads[idx]))
                        .collect::<Vec<_>>();

                    let (abbs, rest): (Vec<_>, Vec<_>) = intervals_of_job_x
                        .chunk_by(|a, b| {
                            a.0.abb
                                .as_ref()
                                .unwrap()
                                .instance_eq(b.0.abb.as_ref().unwrap())
                        })
                        .into_iter() // group by abb
                        .map(|intervals| {
                            (
                                intervals[0].0.abb.as_ref().unwrap().clone(),
                                (
                                    intervals.iter().fold(0, |sum, z| sum + z.0.get_exec_time()),
                                    intervals.iter().fold(Vec::new(), |mut sum, z| {
                                        sum.extend(z.1.iter());
                                        sum
                                    }),
                                ),
                            )
                        })
                        .unzip();
                    let (ticks_per_abb, mem_reads_per_abb): (Vec<_>, Vec<_>) = rest.into_iter().unzip();
                    RTOSJob {
                        name: x.2,
                        mem_reads: mem_reads_per_abb.into_iter().flatten().collect(), // TODO: add read values
                        release: x.0,
                        response: x.1,
                        exec_ticks: ticks_per_abb.iter().sum(),
                        ticks_per_abb: ticks_per_abb,
                        abbs: abbs,
                        hash_cache: 0,
                    }
                })
                .collect::<Vec<_>>();
            jobs
        };
        _state.add_metadata(FreeRTOSTraceMetadata::new(refined_states, intervals, mem_reads, jobs, need_to_debug));
    }

    type ModuleAddressFilter = NopAddressFilter;

    type ModulePageFilter = NopPageFilter;

    fn address_filter(&self) -> &Self::ModuleAddressFilter {
        todo!()
    }

    fn address_filter_mut(&mut self) -> &mut Self::ModuleAddressFilter {
        todo!()
    }

    fn page_filter(&self) -> &Self::ModulePageFilter {
        todo!()
    }

    fn page_filter_mut(&mut self) -> &mut Self::ModulePageFilter {
        todo!()
    }
}

//============================= Trace job response times

pub static mut JOBS_DONE: Vec<(u64, String)> = vec![];

pub fn job_done_hook<QT, S>(
    hooks: &mut EmulatorModules<QT, S>,
    _state: Option<&mut S>,
    _pc: GuestAddr,
) where
    S: UsesInput,
    QT: EmulatorModuleTuple<S>,
{
    let emulator = hooks.qemu();
    let h = hooks
        .modules()
        .match_first_type::<FreeRTOSSystemStateHelper>()
        .expect("QemuSystemHelper not found in helper tupel");
    let curr_tcb_addr: bindings::void_ptr = super::QemuLookup::lookup(&emulator, h.tcb_addr);
    if curr_tcb_addr == 0 {
        return;
    };
    let current_tcb: TCB_t = super::QemuLookup::lookup(&emulator, curr_tcb_addr);
    let tmp = unsafe { std::mem::transmute::<[i8; 10], [u8; 10]>(current_tcb.pcTaskName) };
    let name: String = std::str::from_utf8(&tmp)
        .expect("TCB name was not utf8")
        .chars()
        .filter(|x| *x != '\0')
        .collect::<String>();
    unsafe {
        JOBS_DONE.push((get_icount(&emulator), name));
    }
}

//============================= Trace interrupt service routines

pub fn exec_isr_hook<QT, S>(
    hooks: &mut EmulatorModules<QT, S>,
    _state: Option<&mut S>,
    pc: GuestAddr,
) where
    S: UsesInput,
    QT: EmulatorModuleTuple<S>,
{
    let emulator = hooks.qemu();
    let h = hooks
        .modules()
        .match_first_type::<FreeRTOSSystemStateHelper>()
        .expect("QemuSystemHelper not found in helper tupel");
    let src = read_rec_return_stackframe(&emulator, 0xfffffffc);
    trigger_collection(&emulator, (src, pc), CaptureEvent::ISRStart, h);
    // println!("Exec ISR Call {:#x} {:#x} {}", src, pc, get_icount(emulator));
}

//============================= Trace syscalls and returns

pub fn gen_jmp_is_syscall<QT, S>(
    hooks: &mut EmulatorModules<QT, S>,
    _state: Option<&mut S>,
    src: GuestAddr,
    dest: GuestAddr,
) -> Option<u64>
where
    S: UsesInput,
    QT: EmulatorModuleTuple<S>,
{
    if let Some(h) = hooks
        .modules()
        .match_first_type::<FreeRTOSSystemStateHelper>()
    {
        if h.app_range.contains(&src)
            && !h.app_range.contains(&dest)
            && in_any_range(&h.isr_fn_ranges, src).is_none()
        {
            if let Some(_) = in_any_range(&h.api_fn_ranges, dest) {
                // println!("New jmp {:x} {:x}", src, dest);
                // println!("API Call Edge {:x} {:x}", src, dest);
                return Some(1);
                // TODO: trigger collection right here
                // otherwise there can be a race-condition, where LAST_API_CALL is set before the api starts, if the interrupt handler calls an api function, it will misidentify the callsite of that api call
            }
        } else if dest == 0 {
            // !h.app_range.contains(&src) &&
            if let Some(_) = in_any_range(&h.api_fn_ranges, src) {
                // println!("API Return Edge {:#x}", src);
                return Some(2);
            }
            if let Some(_) = in_any_range(&h.isr_fn_ranges, src) {
                // println!("ISR Return Edge {:#x}", src);
                return Some(3);
            }
        }
    }
    return None;
}

pub fn trace_jmp<QT, S>(
    hooks: &mut EmulatorModules<QT, S>,
    _state: Option<&mut S>,
    src: GuestAddr,
    mut dest: GuestAddr,
    id: u64,
) where
    S: UsesInput,
    QT: EmulatorModuleTuple<S>,
{
    let h = hooks
        .modules()
        .match_first_type::<FreeRTOSSystemStateHelper>()
        .expect("QemuSystemHelper not found in helper tupel");
    let emulator = hooks.qemu();
    if id == 1 {
        // API call
        trigger_collection(&emulator, (src, dest), CaptureEvent::APIStart, h);
        // println!("Exec API Call {:#x} {:#x} {}", src, dest, get_icount(emulator));
    } else if id == 2 {
        // API return
        // Ignore returns to other APIs or ISRs. We only account for the first call depth of API calls from user space.
        if in_any_range(&h.api_fn_ranges, dest).is_none()
            && in_any_range(&h.isr_fn_ranges, dest).is_none()
        {
            let mut edge = (0, 0);
            edge.0 = in_any_range(&h.api_fn_ranges, src).unwrap().start;
            edge.1 = dest;

            trigger_collection(&emulator, edge, CaptureEvent::APIEnd, h);
            // println!("Exec API Return Edge {:#x} {:#x} {}", src, dest, get_icount(emulator));
        }
    } else if id == 3 {
        // ISR return
        dest = read_rec_return_stackframe(&emulator, dest);

        let mut edge = (0, 0);
        edge.0 = in_any_range(&h.isr_fn_ranges, src).unwrap().start;
        edge.1 = dest;

        trigger_collection(&emulator, edge, CaptureEvent::ISREnd, h);
        // println!("Exec ISR Return Edge {:#x} {:#x} {}", src, dest, get_icount(emulator));
    }
}

//============================= Read Hooks
#[allow(unused)]
pub fn gen_read_is_input<QT, S>(
    hooks: &mut EmulatorModules<QT, S>,
    _state: Option<&mut S>,
    pc: GuestAddr,
    _addr: *mut TCGTemp,
    _info: MemAccessInfo,
) -> Option<u64>
where
    S: UsesInput,
    QT: EmulatorModuleTuple<S>,
{
    if let Some(h) = hooks
        .modules()
        .match_first_type::<FreeRTOSSystemStateHelper>()
    {
        if h.app_range.contains(&pc) {
            // println!("gen_read {:x}", pc);
            return Some(1);
        }
    }
    return None;
}

static mut INPUT_MEM: Range<GuestAddr> = 0..0;
pub static mut MEM_READ: Option<Vec<(GuestAddr, u8)>> = None;

#[allow(unused)]
pub fn trace_reads<QT, S>(
    hooks: &mut EmulatorModules<QT, S>,
    _state: Option<&mut S>,
    _id: u64,
    addr: GuestAddr,
    _size: usize,
) where
    S: UsesInput,
    QT: EmulatorModuleTuple<S>,
{
    if unsafe { INPUT_MEM.contains(&addr) } {
        let emulator = hooks.qemu();
        let mut buf: [u8; 1] = [0];
        unsafe {
            emulator.read_mem(addr, &mut buf);
        }
        if unsafe { MEM_READ.is_none() } {
            unsafe { MEM_READ = Some(Vec::from([(addr, buf[0])])) };
        } else {
            unsafe { MEM_READ.as_mut().unwrap().push((addr, buf[0])) };
        }
        // println!("exec_read {:x} {}", addr, size);
    }
}

//============================= Parsing helpers

/// Parse a List_t containing TCB_t into Vec<TCB_t> from cache. Consumes the elements from cache
fn tcb_list_to_vec_cached(list: List_t, dump: &mut HashMap<u32, FreeRTOSStruct>) -> Vec<TCB_t> {
    let mut ret: Vec<TCB_t> = Vec::new();
    if list.uxNumberOfItems == 0 {
        return ret;
    }
    let last_list_item = match dump
        .remove(&list.pxIndex)
        .expect("List_t entry was not in Hashmap")
    {
        List_Item_struct(li) => li,
        List_MiniItem_struct(mli) => match dump
            .remove(&mli.pxNext)
            .expect("MiniListItem pointer invaild")
        {
            List_Item_struct(li) => li,
            _ => panic!("MiniListItem of a non empty List does not point to ListItem"),
        },
        _ => panic!("List_t entry was not a ListItem"),
    };
    let mut next_index = last_list_item.pxNext;
    let last_tcb = match dump
        .remove(&last_list_item.pvOwner)
        .expect("ListItem Owner not in Hashmap")
    {
        TCB_struct(t) => t,
        _ => panic!("List content does not equal type"),
    };
    for _ in 0..list.uxNumberOfItems - 1 {
        let next_list_item = match dump
            .remove(&next_index)
            .expect("List_t entry was not in Hashmap")
        {
            List_Item_struct(li) => li,
            List_MiniItem_struct(mli) => match dump
                .remove(&mli.pxNext)
                .expect("MiniListItem pointer invaild")
            {
                List_Item_struct(li) => li,
                _ => panic!("MiniListItem of a non empty List does not point to ListItem"),
            },
            _ => panic!("List_t entry was not a ListItem"),
        };
        match dump
            .remove(&next_list_item.pvOwner)
            .expect("ListItem Owner not in Hashmap")
        {
            TCB_struct(t) => ret.push(t),
            _ => panic!("List content does not equal type"),
        }
        next_index = next_list_item.pxNext;
    }
    ret.push(last_tcb);
    ret
}

//============================= State refinement

/// Drains a List of raw SystemStates to produce a refined trace
/// returns:
/// - a Vec of FreeRTOSSystemState
/// - a Vec of FreeRTOSSystemStateContext (qemu_tick, (capture_event, capture_name), edge, mem_reads)
fn refine_system_states(
    mut input: Vec<RawFreeRTOSSystemState>,
) -> (Vec<FreeRTOSSystemState>, Vec<FreeRTOSSystemStateContext>) {
    let mut ret = (Vec::<_>::new(), Vec::<_>::new());
    for mut i in input.drain(..) {
        let cur = RefinedTCB::from_tcb_owned(i.current_tcb);
        // println!("Refine: {} {:?} {:?} {:x}-{:x}", cur.task_name, i.capture_point.0, i.capture_point.1.to_string(), i.edge.0, i.edge.1);
        // collect ready list
        let mut collector = Vec::<RefinedTCB>::new();
        for j in i.prio_ready_lists.into_iter().rev() {
            let mut tmp = tcb_list_to_vec_cached(j, &mut i.dumping_ground)
                .iter()
                .map(|x| RefinedTCB::from_tcb(x))
                .collect();
            collector.append(&mut tmp);
        }
        // collect delay list
        let mut delay_list: Vec<RefinedTCB> =
            tcb_list_to_vec_cached(i.delay_list, &mut i.dumping_ground)
                .iter()
                .map(|x| RefinedTCB::from_tcb(x))
                .collect();
        let mut delay_list_overflow: Vec<RefinedTCB> =
            tcb_list_to_vec_cached(i.delay_list_overflow, &mut i.dumping_ground)
                .iter()
                .map(|x| RefinedTCB::from_tcb(x))
                .collect();
        delay_list.append(&mut delay_list_overflow);
        delay_list.sort_by(|a, b| a.task_name.cmp(&b.task_name));

        ret.0.push(FreeRTOSSystemState {
            current_task: cur,
            ready_list_after: collector,
            delay_list_after: delay_list,
            read_invalid: i.read_invalid,
            // input_counter: i.input_counter,//+IRQ_INPUT_BYTES_NUMBER,
        });
        ret.1.push(FreeRTOSSystemStateContext {
            qemu_tick: i.qemu_tick,
            capture_point: (i.capture_point.0, i.capture_point.1),
            edge: i.edge,
            mem_reads: i.mem_reads,
        });
    }
    return ret;
}

/// Transform the states and metadata into a list of ExecIntervals, along with a HashMap of states, a list of HashSets marking memory reads and a bool indicating success
/// returns:
/// - a Vec of ExecIntervals
/// - a Vec of HashSets marking memory reads during these intervals
/// - a HashMap of ReducedFreeRTOSSystemStates by hash
/// - a bool indicating success
fn states2intervals(
    trace: Vec<FreeRTOSSystemState>,
    meta: Vec<FreeRTOSSystemStateContext>,
) -> (
    Vec<ExecInterval>,
    Vec<Vec<(u32, u8)>>,
    HashMap<u64, FreeRTOSSystemState>,
    bool,
) {
    if trace.len() == 0 {
        return (Vec::new(), Vec::new(), HashMap::new(), true);
    }
    let mut isr_stack: VecDeque<u8> = VecDeque::from([]); // 2+ = ISR, 1 = systemcall, 0 = APP. Trace starts with an ISREnd and executes the app

    let mut level_of_task: HashMap<&str, u8> = HashMap::new();

    let mut ret: Vec<ExecInterval> = vec![];
    let mut reads: Vec<Vec<(u32, u8)>> = vec![];
    let mut edges: Vec<(u32, u32)> = vec![];
    let mut last_hash: u64 = compute_hash(&trace[0]);
    let mut table: HashMap<u64, FreeRTOSSystemState> = HashMap::new();
    table.insert(last_hash, trace[0].clone());
    for i in 0..trace.len() - 1 {
        let curr_name = trace[i].current_task().task_name().as_str();
        // let mut interval_name = curr_name;  // Name of the interval, either the task name or the isr/api funtion name
        let level = match meta[i].capture_point.0 {
            CaptureEvent::APIEnd => {
                // API end always exits towards the app
                if !level_of_task.contains_key(curr_name) {
                    level_of_task.insert(curr_name, 0);
                }
                *level_of_task.get_mut(curr_name).unwrap() = 0;
                0
            }
            CaptureEvent::APIStart => {
                // API start can only be called in the app
                if !level_of_task.contains_key(curr_name) {
                    // Should not happen, apps start from an ISR End. Some input exibited this behavior for unknown reasons
                    level_of_task.insert(curr_name, 0);
                }
                *level_of_task.get_mut(curr_name).unwrap() = 1;
                // interval_name = &meta[i].2;
                1
            }
            CaptureEvent::ISREnd => {
                // special case where the next block is an app start
                if !level_of_task.contains_key(curr_name) {
                    level_of_task.insert(curr_name, 0);
                }
                // nested isr, TODO: Test level > 2
                if isr_stack.len() > 1 {
                    // interval_name = ""; // We can't know which isr is running
                    isr_stack.pop_back().unwrap();
                    *isr_stack.back().unwrap()
                } else {
                    isr_stack.pop_back();
                    // possibly go back to an api call that is still running for this task
                    if level_of_task.get(curr_name).unwrap() == &1 {
                        // interval_name = ""; // We can't know which api is running
                    }
                    *level_of_task.get(curr_name).unwrap()
                }
            }
            CaptureEvent::ISRStart => {
                // special case for isrs which do not capture their end
                // if meta[i].2 == "ISR_0_Handler" {
                //     &2
                // } else {
                // regular case
                // interval_name = &meta[i].2;
                if isr_stack.len() > 0 {
                    let l = *isr_stack.back().unwrap();
                    isr_stack.push_back(l + 1);
                    l + 1
                } else {
                    isr_stack.push_back(2);
                    2
                }
                // }
            }
            _ => 100,
        };
        // if trace[i].2 == CaptureEvent::End {break;}
        let next_hash = compute_hash(&trace[i + 1]);
        if !table.contains_key(&next_hash) {
            table.insert(next_hash, trace[i + 1].clone());
        }
        ret.push(ExecInterval {
            start_tick: meta[i].qemu_tick,
            end_tick: meta[i + 1].qemu_tick,
            start_state: last_hash,
            end_state: next_hash,
            start_capture: meta[i].capture_point.clone(),
            end_capture: meta[i + 1].capture_point.clone(),
            level: level,
            abb: None,
        });
        reads.push(meta[i + 1].mem_reads.clone());
        last_hash = next_hash;
        edges.push((meta[i].edge.1, meta[i + 1].edge.0));
    }
    let t = add_abb_info(&mut ret, &table, &edges);
    (ret, reads, table, t)
}

/// Marks which abbs were executed at each interval
fn add_abb_info(
    trace: &mut Vec<ExecInterval>,
    table: &HashMap<u64, FreeRTOSSystemState>,
    edges: &Vec<(u32, u32)>,
) -> bool {
    let mut id_count = 0;
    let mut ret = true;
    let mut task_has_started: HashSet<&String> = HashSet::new();
    let mut wip_abb_trace: Vec<Rc<RefCell<AtomicBasicBlock>>> = vec![];
    // let mut open_abb_at_this_task_or_level : HashMap<(u8,&str),usize> = HashMap::new();
    let mut open_abb_at_this_ret_addr_and_task: HashMap<(u32, &str), usize> = HashMap::new();

    for i in 0..trace.len() {
        let curr_name = table[&trace[i].start_state].current_task().task_name();
        // let last : Option<&usize> = last_abb_start_of_task.get(&curr_name);

        // let open_abb = open_abb_at_this_task_or_level.get(&(trace[i].level, if trace[i].level<2 {&curr_name} else {""})).to_owned();  // apps/apis are differentiated by task name, isrs by nested level
        let open_abb = open_abb_at_this_ret_addr_and_task
            .get(&(edges[i].0, if trace[i].level < 2 { &curr_name } else { "" }))
            .to_owned(); // apps/apis are differentiated by task name, isrs by nested level

        // println!("Edge {:x}-{:x}", edges[i].0.unwrap_or(0xffff), edges[i].1.unwrap_or(0xffff));

        match trace[i].start_capture.0 {
            // generic api abb start
            CaptureEvent::APIStart => {
                // assert_eq!(open_abb, None);
                ret &= open_abb.is_none();
                open_abb_at_this_ret_addr_and_task.insert(
                    (edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }),
                    i,
                );
                wip_abb_trace.push(Rc::new(RefCell::new(AtomicBasicBlock {
                    start: edges[i].0,
                    ends: HashSet::new(),
                    level: if trace[i].level < 2 {
                        trace[i].level
                    } else {
                        2
                    },
                    instance_id: id_count,
                    instance_name: Some(trace[i].start_capture.1.clone()),
                })));
                id_count += 1;
            }
            // generic isr abb start
            CaptureEvent::ISRStart => {
                // assert_eq!(open_abb, None);
                ret &= open_abb.is_none();
                open_abb_at_this_ret_addr_and_task.insert(
                    (edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }),
                    i,
                );
                wip_abb_trace.push(Rc::new(RefCell::new(AtomicBasicBlock {
                    start: edges[i].0,
                    ends: HashSet::new(),
                    level: if trace[i].level < 2 {
                        trace[i].level
                    } else {
                        2
                    },
                    instance_id: id_count,
                    instance_name: Some(trace[i].start_capture.1.clone()),
                })));
                id_count += 1;
            }
            // generic app abb start
            CaptureEvent::APIEnd => {
                // assert_eq!(open_abb, None);
                ret &= open_abb.is_none();
                open_abb_at_this_ret_addr_and_task.insert(
                    (edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }),
                    i,
                );
                wip_abb_trace.push(Rc::new(RefCell::new(AtomicBasicBlock {
                    start: edges[i].0,
                    ends: HashSet::new(),
                    level: if trace[i].level < 2 {
                        trace[i].level
                    } else {
                        2
                    },
                    instance_id: id_count,
                    instance_name: if trace[i].level < 2 {
                        Some(Cow::Owned(curr_name.to_owned()))
                    } else {
                        None
                    },
                })));
                id_count += 1;
            }
            // generic continued blocks
            CaptureEvent::ISREnd => {
                // special case app abb start
                if trace[i].start_capture.1 == "xPortPendSVHandler"
                    && !task_has_started.contains(&curr_name)
                {
                    // assert_eq!(open_abb, None);
                    ret &= open_abb.is_none();
                    wip_abb_trace.push(Rc::new(RefCell::new(AtomicBasicBlock {
                        start: 0,
                        ends: HashSet::new(),
                        level: if trace[i].level < 2 {
                            trace[i].level
                        } else {
                            2
                        },
                        instance_id: id_count,
                        instance_name: Some(Cow::Owned(curr_name.to_owned())),
                    })));
                    id_count += 1;
                    open_abb_at_this_ret_addr_and_task.insert(
                        (edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }),
                        i,
                    );
                    task_has_started.insert(&curr_name);
                } else {
                    if let Some(last) = open_abb_at_this_ret_addr_and_task
                        .get(&(edges[i].0, if trace[i].level < 2 { &curr_name } else { "" }))
                    {
                        let last = last.clone(); // required to drop immutable reference
                        wip_abb_trace.push(wip_abb_trace[last].clone());
                        // if the abb is interrupted again, it will need to continue at edge[i].1
                        open_abb_at_this_ret_addr_and_task.remove(&(
                            edges[i].0,
                            if trace[i].level < 2 { &curr_name } else { "" },
                        ));
                        open_abb_at_this_ret_addr_and_task.insert(
                            (edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }),
                            last,
                        ); // order matters!
                    } else {
                        // panic!();
                        // println!("Continued block with no start {} {} {:?} {:?} {:x}-{:x} {} {}", curr_name, trace[i].start_tick, trace[i].start_capture, trace[i].end_capture, edges[i].0, edges[i].1, task_has_started.contains(curr_name),trace[i].level);
                        // println!("{:x?}", open_abb_at_this_ret_addr_and_task);
                        ret = false;
                        wip_abb_trace.push(Rc::new(RefCell::new(AtomicBasicBlock {
                            start: edges[i].1,
                            ends: HashSet::new(),
                            level: if trace[i].level < 2 {
                                trace[i].level
                            } else {
                                2
                            },
                            instance_id: id_count,
                            instance_name: if trace[i].level < 1 {
                                Some(Cow::Owned(curr_name.to_owned()))
                            } else {
                                None
                            },
                        })));
                        id_count += 1;
                    }
                }
            }
            _ => panic!("Undefined block start"),
        }
        match trace[i].end_capture.0 {
            // generic app abb end
            CaptureEvent::APIStart => {
                let _t = &wip_abb_trace[i];
                RefCell::borrow_mut(&*wip_abb_trace[i])
                    .ends
                    .insert(edges[i].1);
                open_abb_at_this_ret_addr_and_task
                    .remove(&(edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }));
            }
            // generic api abb end
            CaptureEvent::APIEnd => {
                RefCell::borrow_mut(&*wip_abb_trace[i])
                    .ends
                    .insert(edges[i].1);
                open_abb_at_this_ret_addr_and_task
                    .remove(&(edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }));
            }
            // generic isr abb end
            CaptureEvent::ISREnd => {
                RefCell::borrow_mut(&*wip_abb_trace[i])
                    .ends
                    .insert(edges[i].1);
                open_abb_at_this_ret_addr_and_task
                    .remove(&(edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }));
            }
            // end anything
            CaptureEvent::End => {
                RefCell::borrow_mut(&*wip_abb_trace[i])
                    .ends
                    .insert(edges[i].1);
                open_abb_at_this_ret_addr_and_task
                    .remove(&(edges[i].1, if trace[i].level < 2 { &curr_name } else { "" }));
            }
            CaptureEvent::ISRStart => (),
            _ => panic!("Undefined block end"),
        }
        // println!("{} {} {:x}-{:x} {:x}-{:x} {:?} {:?} {}",curr_name, trace[i].level, edges[i].0, edges[i].1, ((*wip_abb_trace[i])).borrow().start, ((*wip_abb_trace[i])).borrow().ends.iter().next().unwrap_or(&0xffff), trace[i].start_capture, trace[i].end_capture, trace[i].start_tick);
        // println!("{:x?}", open_abb_at_this_ret_addr_and_task);
    }
    // drop(open_abb_at_this_task_or_level);

    for i in 0..trace.len() {
        trace[i].abb = Some((*wip_abb_trace[i]).borrow().clone());
    }
    return ret;
}

//============================================= Task release times

// Find all task release times.
fn get_releases(
    trace: &Vec<ExecInterval>,
    states: &HashMap<u64, FreeRTOSSystemState>,
) -> Vec<(u64, String)> {
    let mut ret = Vec::new();
    let mut initial_released = false;
    for (_n, i) in trace.iter().enumerate() {
        // The first release starts from xPortPendSVHandler
        if !initial_released
            && i.start_capture.0 == CaptureEvent::ISREnd
            && i.start_capture.1 == "xPortPendSVHandler"
        {
            let start_state = states.get(&i.start_state).expect("State not found");
            initial_released = true;
            start_state.get_ready_lists().iter().for_each(|x| {
                ret.push((i.start_tick, x.task_name().clone()));
            });
            continue;
        }
        // A timed release is SysTickHandler isr block that moves a task from the delay list to the ready list.
        if i.start_capture.0 == CaptureEvent::ISRStart
            && (i.start_capture.1 == "xPortSysTickHandler"
                || USR_ISR_SYMBOLS.contains(&&*i.start_capture.1))
        {
            // detect race-conditions, get start and end state from the nearest valid intervals
            if states
                .get(&i.start_state)
                .map(|x| x.read_invalid)
                .unwrap_or(true)
            {
                let mut start_index = None;
                for n in 1.._n {
                    if let Some(interval_start) = trace.get(_n - n) {
                        let start_state = states.get(&interval_start.start_state).unwrap();
                        if !start_state.read_invalid {
                            start_index = Some(_n - n);
                            break;
                        }
                    } else {
                        break;
                    }
                }
                let mut end_index = None;
                for n in (_n + 1)..trace.len() {
                    if let Some(interval_end) = trace.get(n) {
                        let end_state = states.get(&interval_end.end_state).unwrap();
                        if !end_state.read_invalid {
                            end_index = Some(n);
                            break;
                        }
                    } else {
                        break;
                    }
                }
                if let Some(Some(start_state)) =
                    start_index.map(|x| states.get(&trace[x].start_state))
                {
                    if let Some(Some(end_state)) =
                        end_index.map(|x| states.get(&trace[x].end_state))
                    {
                        end_state.ready_list_after.iter().for_each(|x| {
                            if x.task_name != end_state.current_task.task_name
                                && x.task_name != start_state.current_task.task_name
                                && !start_state
                                    .ready_list_after
                                    .iter()
                                    .any(|y| x.task_name == y.task_name)
                            {
                                ret.push((i.end_tick, x.task_name.clone()));
                            }
                        });
                    }
                }
            } else
            // canonical case, userspace -> isr -> userspace
            if i.end_capture.0 == CaptureEvent::ISREnd {
                let start_state = states.get(&i.start_state).expect("State not found");
                let end_state = states.get(&i.end_state).expect("State not found");
                end_state.ready_list_after.iter().for_each(|x| {
                    if x.task_name != end_state.current_task.task_name
                        && x.task_name != start_state.current_task.task_name
                        && !start_state
                            .ready_list_after
                            .iter()
                            .any(|y| x.task_name == y.task_name)
                    {
                        ret.push((i.end_tick, x.task_name.clone()));
                    }
                });
            // start_state.delay_list_after.iter().for_each(|x| {
            //     if !end_state.delay_list_after.iter().any(|y| x.task_name == y.task_name) {
            //         ret.push((i.end_tick, x.task_name.clone()));
            //     }
            // });
            } else if i.end_capture.0 == CaptureEvent::ISRStart {
                // Nested interrupts. Fast-forward to the end of the original interrupt, or the first valid state thereafter
                // TODO: this may cause the same release to be registered multiple times
                let mut isr_has_ended = false;
                let start_state = states.get(&i.start_state).expect("State not found");
                for n in (_n + 1)..trace.len() {
                    if let Some(interval_end) = trace.get(n) {
                        if interval_end.end_capture.1 == i.start_capture.1 || isr_has_ended {
                            let end_state = states.get(&interval_end.end_state).unwrap();
                            isr_has_ended = true;
                            if !end_state.read_invalid {
                                end_state.ready_list_after.iter().for_each(|x| {
                                    if x.task_name != end_state.current_task.task_name
                                        && x.task_name != start_state.current_task.task_name
                                        && !start_state
                                            .ready_list_after
                                            .iter()
                                            .any(|y| x.task_name == y.task_name)
                                    {
                                        ret.push((i.end_tick, x.task_name.clone()));
                                    }
                                });
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
                // if let Some(interval_end) = trace.get(_n+2) {
                //     if interval_end.start_capture.0 == CaptureEvent::ISREnd && interval_end.end_capture.0 == CaptureEvent::ISREnd && interval_end.end_capture.1 == i.start_capture.1 {
                //         let start_state = states.get(&i.start_state).expect("State not found");
                //         let end_state = states.get(&interval_end.end_state).expect("State not found");
                //         end_state.ready_list_after.iter().for_each(|x| {
                //             if x.task_name != end_state.current_task.task_name && x.task_name != start_state.current_task.task_name && !start_state.ready_list_after.iter().any(|y| x.task_name == y.task_name) {
                //                 ret.push((i.end_tick, x.task_name.clone()));
                //             }
                //         });
                //     }
                // }
            }
        }
        // Release driven by an API call. This produces a lot of false positives, as a job may block multiple times per instance. Despite this, aperiodic jobs not be modeled otherwise. If we assume the first release is the real one, we can filter out the rest.
        if i.start_capture.0 == CaptureEvent::APIStart {
            let api_start_state = states.get(&i.start_state).expect("State not found");
            let api_end_state = {
                let mut end_index = _n;
                for n in (_n)..trace.len() {
                    if trace[n].end_capture.0 == CaptureEvent::APIEnd
                        || trace[n].end_capture.0 == CaptureEvent::End
                    {
                        end_index = n;
                        break;
                    } else if n > _n && trace[n].level == 0 {
                        // API Start -> ISR Start+End -> APP Continue
                        end_index = n - 1; // any return to a regular app block is a fair point of comparison for the ready list, because scheduling has been performed
                        break;
                    }
                }
                states
                    .get(&trace[end_index].end_state)
                    .expect("State not found")
            };
            api_end_state.ready_list_after.iter().for_each(|x| {
                if x.task_name != api_start_state.current_task.task_name
                    && !api_start_state
                        .ready_list_after
                        .iter()
                        .any(|y| x.task_name == y.task_name)
                {
                    ret.push((i.end_tick, x.task_name.clone()));
                    // eprintln!("Task {} released by API call at {:.1}ms", x.task_name, crate::time::clock::tick_to_time(i.end_tick).as_micros() as f32/1000.0);
                }
            });
        }
    }
    ret
}

fn get_release_response_pairs(
    rel: &Vec<(u64, String)>,
    resp: &Vec<(u64, String)>,
) -> (Vec<(u64, u64, String)>, bool) {
    let mut maybe_error = false;
    let mut ret = Vec::new();
    let mut ready: HashMap<&String, u64> = HashMap::new();
    let mut last_response: HashMap<&String, u64> = HashMap::new();
    let mut r = rel.iter().peekable();
    let mut d = resp.iter().peekable();
    loop {
        while let Some(peek_rel) = r.peek() {
            // Fill releases as soon as possible
            if !ready.contains_key(&peek_rel.1) {
                ready.insert(&peek_rel.1, peek_rel.0);
                r.next();
            } else {
                if let Some(peek_resp) = d.peek() {
                    if peek_resp.0 > peek_rel.0 {
                        // multiple releases before response
                        // It is unclear which release is real
                        // maybe_error = true;
                        // eprintln!("Task {} released multiple times before response ({:.1}ms and {:.1}ms)", peek_rel.1, crate::time::clock::tick_to_time(ready[&peek_rel.1]).as_micros()/1000, crate::time::clock::tick_to_time(peek_rel.0).as_micros()/1000);
                        // ready.insert(&peek_rel.1, peek_rel.0);
                        r.next();
                    } else {
                        // releases have overtaken responses, wait until the ready list clears up a bit
                        break;
                    }
                } else {
                    // no more responses
                    break;
                }
            }
        }
        if let Some(next_resp) = d.next() {
            if ready.contains_key(&next_resp.1) {
                if ready[&next_resp.1] >= next_resp.0 {
                    if let Some(lr) = last_response.get(&next_resp.1) {
                        if u128::abs_diff(
                            crate::time::clock::tick_to_time(next_resp.0).as_micros(),
                            crate::time::clock::tick_to_time(*lr).as_micros(),
                        ) > 500
                        {
                            // tolerate pending notifications for 500us
                            maybe_error = true;
                            // eprintln!("Task {} response at {:.1}ms before next release at {:.1}ms. Fallback to last response at {:.1}ms.", next_resp.1, crate::time::clock::tick_to_time(next_resp.0).as_micros() as f32/1000.0, crate::time::clock::tick_to_time(ready[&next_resp.1]).as_micros() as f32/1000.0, crate::time::clock::tick_to_time(*lr).as_micros() as f32/1000.0);
                        }
                        // Sometimes a task is released immediately after a response. This might not be detected.
                        // Assume that the release occured with the last response
                        ret.push((*lr, next_resp.0, next_resp.1.clone()));
                        last_response.insert(&next_resp.1, next_resp.0);
                    } else {
                        maybe_error = true;
                        // eprintln!("Task {} released after response", next_resp.1);
                    }
                } else {
                    // assert!(peek_resp.0 >= ready[&peek_resp.1]);
                    last_response.insert(&next_resp.1, next_resp.0);
                    ret.push((ready[&next_resp.1], next_resp.0, next_resp.1.clone()));
                    ready.remove(&next_resp.1);
                }
            } else {
                if let Some(lr) = last_response.get(&next_resp.1) {
                    if u128::abs_diff(
                        crate::time::clock::tick_to_time(next_resp.0).as_micros(),
                        crate::time::clock::tick_to_time(*lr).as_micros(),
                    ) > 1000
                    { // tolerate pending notifications for 1ms
                         // maybe_error = true;
                         // eprintln!("Task {} response at {:.1}ms not found in ready list. Fallback to last response at {:.1}ms.", next_resp.1, crate::time::clock::tick_to_time(next_resp.0).as_micros() as f32/1000.0, crate::time::clock::tick_to_time(*lr).as_micros() as f32/1000.0);
                    }
                    // Sometimes a task is released immediately after a response (e.g. pending notification). This might not be detected.
                    // Assume that the release occured with the last response
                    ret.push((*lr, next_resp.0, next_resp.1.clone()));
                    last_response.insert(&next_resp.1, next_resp.0);
                } else {
                    maybe_error = true;
                    // eprintln!("Task {} response at {:.1}ms not found in ready list", next_resp.1, crate::time::clock::tick_to_time(next_resp.0).as_micros() as f32/1000.0);
                }
            }
        } else {
            // TODO: should remaining released tasks be counted as finished?
            return (ret, maybe_error);
        }
    }
}
