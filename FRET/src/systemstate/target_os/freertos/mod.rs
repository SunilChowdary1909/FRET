use libafl_qemu::GuestAddr;
use qemu_module::{FreeRTOSSystemStateHelper, MEM_READ};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::{
    impl_emu_lookup,
    systemstate::{helpers::get_icount, CaptureEvent},
};

pub mod bindings;
pub mod qemu_module;
pub mod config;
use bindings::*;

use super::QemuLookup;
use crate::systemstate::target_os::*;

// Constants
const NUM_PRIOS: usize = 15;

//============================================================================= Outside interface

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FreeRTOSSystem {
    pub raw_trace: Vec<RawFreeRTOSSystemState>,
}

impl TargetSystem for FreeRTOSSystem {
    type State = FreeRTOSSystemState;
    type TCB = RefinedTCB;
    type TraceData = FreeRTOSTraceMetadata;
}

impl TaskControlBlock for RefinedTCB {
    fn task_name(&self) -> &String {
        &self.task_name
    }
    fn task_name_mut(&mut self) -> &mut String {
        &mut self.task_name
    }
}

impl SystemState for FreeRTOSSystemState {
    type TCB = RefinedTCB;

    fn current_task(&self) -> &Self::TCB {
        &self.current_task
    }

    fn get_ready_lists(&self) -> &Vec<Self::TCB> {
        &self.ready_list_after
    }

    fn get_delay_list(&self) -> &Vec<Self::TCB> {
        &self.delay_list_after
    }

    fn print_lists(&self) -> String {
        self.print_lists()  
    }
    
    fn current_task_mut(&mut self) -> &mut Self::TCB {
        &mut self.current_task
    }
}

//============================================================================= Data structures

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum FreeRTOSStruct {
    TCB_struct(TCB_t),
    List_struct(List_t),
    List_Item_struct(ListItem_t),
    List_MiniItem_struct(MiniListItem_t),
}

impl_emu_lookup!(TCB_t);
impl_emu_lookup!(List_t);
impl_emu_lookup!(ListItem_t);
impl_emu_lookup!(MiniListItem_t);
impl_emu_lookup!(void_ptr);
impl_emu_lookup!(TaskStatus_t);

pub const ISR_SYMBOLS: &'static [&'static str] = &[
    // ISRs
    "Reset_Handler",
    "Default_Handler",
    "Default_Handler2",
    "Default_Handler3",
    "Default_Handler4",
    "Default_Handler5",
    "Default_Handler6",
    "vPortSVCHandler",
    "xPortPendSVHandler",
    "xPortSysTickHandler",
    "ISR_0_Handler",
    "ISR_1_Handler",
    "ISR_2_Handler",
    "ISR_3_Handler",
    "ISR_4_Handler",
    "ISR_5_Handler",
    "ISR_6_Handler",
    "ISR_7_Handler",
    "ISR_8_Handler",
    "ISR_9_Handler",
    "ISR_10_Handler",
    "ISR_11_Handler",
    "ISR_12_Handler",
    "ISR_13_Handler",
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
    "ISR_8_Handler",
    "ISR_9_Handler",
    "ISR_10_Handler",
    "ISR_11_Handler",
    "ISR_12_Handler",
    "ISR_13_Handler",
];

//============================================================================= Helper functions

/// Reads a FreeRTOS list from the target and populates the system state.
///
/// # Arguments
/// * `systemstate` - The mutable system state to populate.
/// * `emulator` - The QEMU emulator instance.
/// * `target` - The address of the list to read.
///
/// # Returns
/// A tuple containing the read list and a boolean indicating if the read was valid.
fn read_freertos_list(
    systemstate: &mut RawFreeRTOSSystemState,
    emulator: &libafl_qemu::Qemu,
    target: GuestAddr,
) -> (List_t, bool) {
    let read: List_t = QemuLookup::lookup(emulator, target);
    let listbytes: GuestAddr = GuestAddr::try_from(std::mem::size_of::<List_t>()).unwrap();

    let mut next_index = read.pxIndex;
    for _j in 0..read.uxNumberOfItems {
        // always jump over the xListEnd marker
        if (target..target + listbytes).contains(&next_index) {
            let next_item: MiniListItem_t = QemuLookup::lookup(emulator, next_index);
            let new_next_index = next_item.pxNext;
            systemstate
                .dumping_ground
                .insert(next_index, FreeRTOSStruct::List_MiniItem_struct(next_item));
            next_index = new_next_index;
        }
        let next_item: ListItem_t = QemuLookup::lookup(emulator, next_index);
        // println!("Item at {}: {:?}",next_index,next_item);
        if next_item.pvContainer != target {
            // the list is being modified, abort by setting the list empty
            eprintln!("Warning: attempted to read a list that is being modified");
            let mut read = read;
            read.uxNumberOfItems = 0;
            return (read, false);
        }
        // assert_eq!(next_item.pvContainer,target);
        let new_next_index = next_item.pxNext;
        let next_tcb: TCB_t = QemuLookup::lookup(emulator, next_item.pvOwner);
        // println!("TCB at {}: {:?}",next_item.pvOwner,next_tcb);
        systemstate.dumping_ground.insert(
            next_item.pvOwner,
            FreeRTOSStruct::TCB_struct(next_tcb.clone()),
        );
        systemstate
            .dumping_ground
            .insert(next_index, FreeRTOSStruct::List_Item_struct(next_item));
        next_index = new_next_index;
    }
    // Handle edge case where the end marker was not included yet
    if (target..target + listbytes).contains(&next_index) {
        let next_item: freertos::MiniListItem_t = QemuLookup::lookup(emulator, next_index);
        systemstate
            .dumping_ground
            .insert(next_index, FreeRTOSStruct::List_MiniItem_struct(next_item));
    }
    return (read, true);
}

/// Triggers the collection of a FreeRTOS system state snapshot at a given event.
///
/// # Arguments
/// * `emulator` - The QEMU emulator instance.
/// * `edge` - A tuple of (from, to) addresses representing the edge.
/// * `event` - The capture event type.
/// * `h` - The FreeRTOS system state helper.
#[inline]
fn trigger_collection(
    emulator: &libafl_qemu::Qemu,
    edge: (GuestAddr, GuestAddr),
    event: CaptureEvent,
    h: &FreeRTOSSystemStateHelper,
) {
    let listbytes: GuestAddr =
        GuestAddr::try_from(std::mem::size_of::<freertos::List_t>()).unwrap();
    let mut systemstate = RawFreeRTOSSystemState::default();

    match event {
        CaptureEvent::APIStart => {
            let s : &Cow<'static, str> = h.api_fn_addrs.get(&edge.1).unwrap();
            systemstate.capture_point = (CaptureEvent::APIStart, s.clone());
        }
        CaptureEvent::APIEnd => {
            let s : &Cow<'static, str> = h.api_fn_addrs.get(&edge.0).unwrap();
            systemstate.capture_point = (CaptureEvent::APIEnd, s.clone());
        }
        CaptureEvent::ISRStart => {
            let s : &Cow<'static, str> = h.isr_fn_addrs.get(&edge.1).unwrap();
            systemstate.capture_point = (CaptureEvent::ISRStart, s.clone());
        }
        CaptureEvent::ISREnd => {
            let s : &Cow<'static, str> = h.isr_fn_addrs.get(&edge.0).unwrap();
            systemstate.capture_point = (CaptureEvent::ISREnd, s.clone());
        }
        CaptureEvent::End => {
            systemstate.capture_point = (CaptureEvent::End, Cow::Borrowed(""));
        }
        CaptureEvent::Undefined => (),
    }

    if systemstate.capture_point.0 == CaptureEvent::Undefined {
        // println!("Not found: {:#x} {:#x}", edge.0.unwrap_or(0), edge.1.unwrap_or(0));
    }
    systemstate.edge = ((edge.0), (edge.1));

    systemstate.qemu_tick = get_icount(emulator);

    let curr_tcb_addr: freertos::void_ptr = QemuLookup::lookup(emulator, h.tcb_addr);
    if curr_tcb_addr == 0 {
        return;
    };

    // println!("{:?}",std::str::from_utf8(&current_tcb.pcTaskName));
    let critical: void_ptr = QemuLookup::lookup(emulator, h.critical_addr);
    let suspended: void_ptr = QemuLookup::lookup(emulator, h.scheduler_lock_addr);
    let _running: void_ptr = QemuLookup::lookup(emulator, h.scheduler_running_addr);

    systemstate.current_tcb = QemuLookup::lookup(emulator, curr_tcb_addr);
    // During ISRs it is only safe to extract structs if they are not currently being modified
    if systemstate.capture_point.0 == CaptureEvent::APIStart
        || systemstate.capture_point.0 == CaptureEvent::APIEnd
        || (critical == 0 && suspended == 0)
    {
        // Extract delay list
        let mut target: GuestAddr = h.delay_queue;
        target = QemuLookup::lookup(emulator, target);
        let _temp = read_freertos_list(&mut systemstate, emulator, target);
        systemstate.delay_list = _temp.0;
        systemstate.read_invalid |= !_temp.1;

        // Extract delay list overflow
        let mut target: GuestAddr = h.delay_queue_overflow;
        target = QemuLookup::lookup(emulator, target);
        let _temp = read_freertos_list(&mut systemstate, emulator, target);
        systemstate.delay_list_overflow = _temp.0;
        systemstate.read_invalid |= !_temp.1;

        // Extract suspended tasks (infinite wait), seems broken, always appreas to be modified
        // let mut target : GuestAddr = h.suspended_queue;
        // target = QemuLookup::lookup(emulator, target);
        // systemstate.suspended_list = read_freertos_list(&mut systemstate, emulator, target);

        // Extract priority lists
        for i in 0..NUM_PRIOS {
            let target: GuestAddr = listbytes * GuestAddr::try_from(i).unwrap() + h.ready_queues;
            let _temp = read_freertos_list(&mut systemstate, emulator, target);
            systemstate.prio_ready_lists[i] = _temp.0;
            systemstate.read_invalid |= !_temp.1;
        }
    } else {
        systemstate.read_invalid = true;
    }
    systemstate.mem_reads = unsafe { MEM_READ.take().unwrap_or_default() };

    unsafe {
        CURRENT_SYSTEMSTATE_VEC.push(systemstate);
    }
}

/// Raw info Dump from Qemu
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RawFreeRTOSSystemState {
    qemu_tick: u64,
    current_tcb: TCB_t,
    prio_ready_lists: [freertos::List_t; NUM_PRIOS],
    delay_list: freertos::List_t,
    delay_list_overflow: freertos::List_t,
    dumping_ground: HashMap<u32, freertos::FreeRTOSStruct>,
    read_invalid: bool,
    input_counter: u32,
    edge: (GuestAddr, GuestAddr),
    capture_point: (CaptureEvent, Cow<'static, str>),
    mem_reads: Vec<(u32, u8)>,
}
/// List of system state dumps from EmulatorModules
static mut CURRENT_SYSTEMSTATE_VEC: Vec<RawFreeRTOSSystemState> = vec![];

/// A reduced version of freertos::TCB_t
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct RefinedTCB {
    pub task_name: String,
    pub priority: u32,
    pub base_priority: u32,
    mutexes_held: u32,
    notify_value: u32,
    notify_state: u8,
}

impl PartialEq for RefinedTCB {
    fn eq(&self, other: &Self) -> bool {
        let ret = self.task_name == other.task_name
            && self.priority == other.priority
            && self.base_priority == other.base_priority;
        #[cfg(feature = "do_hash_notify_state")]
        let ret = ret && self.notify_state == other.notify_state;
        #[cfg(feature = "do_hash_notify_value")]
        let ret = ret && self.notify_state == other.notify_state;
        ret
    }
}

impl Hash for RefinedTCB {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.task_name.hash(state);
        self.priority.hash(state);
        self.mutexes_held.hash(state);
        #[cfg(feature = "do_hash_notify_state")]
        self.notify_state.hash(state);
        #[cfg(feature = "do_hash_notify_value")]
        self.notify_value.hash(state);
    }
}

impl RefinedTCB {
    /// Constructs a `RefinedTCB` from a raw FreeRTOS TCB struct reference.
    ///
    /// # Arguments
    /// * `input` - Reference to a raw TCB_t struct.
    ///
    /// # Returns
    /// A new `RefinedTCB` instance.
    pub fn from_tcb(input: &TCB_t) -> Self {
        unsafe {
            let tmp = std::mem::transmute::<[i8; 10], [u8; 10]>(input.pcTaskName);
            let name: String = std::str::from_utf8(&tmp)
                .expect("TCB name was not utf8")
                .chars()
                .filter(|x| *x != '\0')
                .collect::<String>();
            Self {
                task_name: name,
                priority: input.uxPriority,
                base_priority: input.uxBasePriority,
                mutexes_held: input.uxMutexesHeld,
                notify_value: input.ulNotifiedValue[0],
                notify_state: input.ucNotifyState[0],
            }
        }
    }
    /// Constructs a `RefinedTCB` from a raw FreeRTOS TCB struct (by value).
    ///
    /// # Arguments
    /// * `input` - The TCB_t struct.
    ///
    /// # Returns
    /// A new `RefinedTCB` instance.
    pub fn from_tcb_owned(input: TCB_t) -> Self {
        unsafe {
            let tmp = std::mem::transmute::<[i8; 10], [u8; 10]>(input.pcTaskName);
            let name: String = std::str::from_utf8(&tmp)
                .expect("TCB name was not utf8")
                .chars()
                .filter(|x| *x != '\0')
                .collect::<String>();
            Self {
                task_name: name,
                priority: input.uxPriority,
                base_priority: input.uxBasePriority,
                mutexes_held: input.uxMutexesHeld,
                notify_value: input.ulNotifiedValue[0],
                notify_state: input.ucNotifyState[0],
            }
        }
    }
}

/// Reduced information about a systems state, without any execution context
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct FreeRTOSSystemState {
    current_task: RefinedTCB,
    ready_list_after: Vec<RefinedTCB>,
    delay_list_after: Vec<RefinedTCB>,
    read_invalid: bool,
}
impl PartialEq for FreeRTOSSystemState {
    fn eq(&self, other: &Self) -> bool {
        self.current_task == other.current_task
            && self.ready_list_after == other.ready_list_after
            && self.delay_list_after == other.delay_list_after
            && self.read_invalid == other.read_invalid
    }
}

impl Hash for FreeRTOSSystemState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.current_task.hash(state);
        self.ready_list_after.hash(state);
        self.delay_list_after.hash(state);
        self.read_invalid.hash(state);
    }
}
impl FreeRTOSSystemState {
    /// Prints the ready and delay lists as a formatted string.
    ///
    /// # Returns
    /// A string representation of the ready and delay lists.
    pub fn print_lists(&self) -> String {
        let mut ret = String::from("+");
        for j in self.ready_list_after.iter() {
            ret.push_str(format!(" {}", j.task_name).as_str());
        }
        ret.push_str("\n-");
        for j in self.delay_list_after.iter() {
            ret.push_str(format!(" {}", j.task_name).as_str());
        }
        ret
    }
    /// Computes a hash for the system state.
    ///
    /// # Returns
    /// The hash value as a u64.
    pub fn get_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
}

impl fmt::Display for FreeRTOSSystemState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ready = self
            .ready_list_after
            .iter()
            .map(|x| x.task_name.clone())
            .collect::<Vec<_>>()
            .join(" ");
        let delay = self
            .delay_list_after
            .iter()
            .map(|x| x.task_name.clone())
            .collect::<Vec<_>>()
            .join(" ");
        write!(
            f,
            "Valid: {} | Current: {} | Ready: {} | Delay: {}",
            u32::from(!self.read_invalid),
            self.current_task.task_name,
            ready,
            delay
        )
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub(crate)struct FreeRTOSSystemStateContext {
    pub qemu_tick: u64,
    pub capture_point: (CaptureEvent, Cow<'static, str>),
    pub edge: (GuestAddr, GuestAddr),
    pub mem_reads: Vec<(u32, u8)>,
}


#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct FreeRTOSTraceMetadata
{
    trace_map: HashMap<u64, <FreeRTOSTraceMetadata as SystemTraceData>::State>,
    intervals: Vec<ExecInterval>,
    mem_reads: Vec<Vec<(u32, u8)>>,
    jobs: Vec<RTOSJob>,
    trace_length: usize,
    indices: Vec<usize>, // Hashed enumeration of States
    tcref: isize,
    need_to_debug: bool,
}
impl FreeRTOSTraceMetadata
{
    /// Constructs a new `FreeRTOSTraceMetadata` from trace data.
    ///
    /// # Arguments
    /// * `trace` - Vector of system states.
    /// * `intervals` - Vector of execution intervals.
    /// * `mem_reads` - Vector of memory reads.
    /// * `jobs` - Vector of RTOS jobs.
    /// * `need_to_debug` - Whether the current trace should be dumped for debugging purposes.
    ///
    /// # Returns
    /// A new `FreeRTOSTraceMetadata` instance.
    pub fn new(trace: Vec<<FreeRTOSTraceMetadata as SystemTraceData>::State>, intervals: Vec<ExecInterval>, mem_reads: Vec<Vec<(u32, u8)>>, jobs: Vec<RTOSJob>, need_to_debug: bool) -> Self {
        let hashes : Vec<_> = trace
            .iter()
            .map(|x| compute_hash(&x) as usize)
            .collect();
        let trace_map = HashMap::from_iter(trace.into_iter().zip(hashes.iter()).map(|(x, y)| (*y as u64, x)));
        Self {
            trace_length: hashes.len(),  // TODO make this configurable
            trace_map: trace_map,
            intervals: intervals,
            mem_reads: mem_reads,
            jobs: jobs,
            indices: hashes,
            tcref: 0,
            need_to_debug: need_to_debug,
        }
    }
}

impl HasRefCnt for FreeRTOSTraceMetadata
{
    fn refcnt(&self) -> isize {
        self.tcref
    }

    fn refcnt_mut(&mut self) -> &mut isize {
        &mut self.tcref
    }
}

impl SystemTraceData for FreeRTOSTraceMetadata
{
    type State = FreeRTOSSystemState;

    fn states(&self) -> Vec<&Self::State> {
        self.indices.iter().filter_map(|x| self.trace_map.get(&(*x as u64))).collect()
    }

    fn intervals(&self) -> &Vec<ExecInterval> {
        &self.intervals
    }

    fn jobs(&self) -> &Vec<RTOSJob> {
        &self.jobs
    }

    fn trace_length(&self) -> usize {
        self.trace_length
    }
    
    fn mem_reads(&self) -> &Vec<Vec<(u32, u8)>> {
        &self.mem_reads
    }
    
    fn states_map(&self) -> &HashMap<u64, Self::State> {
        &self.trace_map
    }
    
    fn need_to_debug(&self) -> bool {
        self.need_to_debug
    }
}

libafl_bolts::impl_serdeany!(FreeRTOSTraceMetadata);
libafl_bolts::impl_serdeany!(RefinedTCB);
libafl_bolts::impl_serdeany!(FreeRTOSSystemState);
libafl_bolts::impl_serdeany!(FreeRTOSSystem);

/// Returns a set of all task names present in the given trace.
///
/// # Arguments
/// * `trace` - A vector of FreeRTOS system states.
///
/// # Returns
/// A set of unique task names as strings.
pub(crate) fn get_task_names(trace: &Vec<FreeRTOSSystemState>) -> HashSet<String> {
    let mut ret: HashSet<_, _> = HashSet::new();
    for state in trace {
        ret.insert(state.current_task.task_name.to_string());
    }
    ret
}
