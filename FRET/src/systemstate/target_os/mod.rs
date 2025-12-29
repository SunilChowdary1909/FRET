use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use hashbrown::HashSet;
use libafl_bolts::prelude::SerdeAny;
use libafl_bolts::HasRefCnt;
use libafl_qemu::Qemu;
use std::hash::Hasher;
use std::hash::Hash;
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use itertools::Itertools;
use std::fmt::Debug;

use super::helpers::abb_profile;
use super::ExecInterval;
use super::RTOSJob;

#[cfg(feature = "freertos")]
pub mod freertos;

pub mod osek;

//============================= Trait definitions

/// A trait representing a target system, which includes a system state, task control block, and trace data.
pub trait TargetSystem: Serialize + Sized + for<'a> Deserialize<'a> + Default + Debug + Clone + SerdeAny {
    type State: SystemState<TCB = Self::TCB>;
    /// The type of a task control block used in the system state.
    type TCB: TaskControlBlock;
    /// The type used to store trace data for the system.
    type TraceData: SystemTraceData<State = Self::State>;
}

/// A trait representing the system state of a target system, which includes methods to access the current task.
pub trait SystemState: Serialize + Sized + for<'a> Deserialize<'a> + Default + Debug + Hash + PartialEq + Clone + SerdeAny {
    type TCB: TaskControlBlock;

    fn current_task(&self) -> &Self::TCB;
    fn current_task_mut(&mut self) -> &mut Self::TCB;
    fn get_ready_lists(&self) -> &Vec<Self::TCB>;
    fn get_delay_list(&self) -> &Vec<Self::TCB>;
    fn print_lists(&self) -> String;
}

pub trait SystemTraceData: Serialize + Sized + for<'a> Deserialize<'a> + Default + Debug + Clone + SerdeAny + HasRefCnt {
    type State: SystemState;

    /// Returns a vector of all system states in the trace.
    fn states(&self) -> Vec<&Self::State>;
    /// Returns hash map of system states, where the key is the hash value of the state.
    fn states_map(&self) -> &HashMap<u64, Self::State>;
    /// Returns a vector of execution intervals in the trace.
    fn intervals(&self) -> &Vec<ExecInterval>;
    /// Returns a vector of memory reads, where each read is represented as a tuple of (address, value).
    fn mem_reads(&self) -> &Vec<Vec<(u32, u8)>>;
    /// Returns a vector of RTOS jobs which were executed during the trace.
    fn jobs(&self) -> &Vec<RTOSJob>;
    fn trace_length(&self) -> usize;

    #[inline]
    /// Returns the worst job of each task by a given predicate.
    fn worst_jobs_per_task_by(&self, pred: &dyn Fn(&RTOSJob,&RTOSJob) -> bool) -> HashMap<String, RTOSJob> {
        self.jobs().iter().fold(HashMap::new(), |mut acc, next| {
            match acc.get_mut(&next.name) {
                Some(old) => {
                    if pred(old,next) {
                        *old=next.clone();
                    }
                },
                Option::None => {
                    acc.insert(next.name.clone(), next.clone());
                }
            }
            acc
        })
    }
    #[inline]
    /// Gives the worst job of each task by execution time.
    fn worst_jobs_per_task_by_exec_time(&self) -> HashMap<String, RTOSJob> {
        self.worst_jobs_per_task_by(&|old, x| x.exec_ticks > old.exec_ticks)
    }
    #[inline]
    /// Gives the worst job of each task by response time.
    fn worst_jobs_per_task_by_response_time(&self) -> HashMap<String, RTOSJob> {
        self.worst_jobs_per_task_by(&|old, x| x.response_time() > old.response_time())
    }
    #[inline]
    /// Gives the response time of the worst job of the selected task, or 0 if the task is not found
    fn wort_of_task(&self, select_task: &String) -> u64 {
        self.worst_jobs_per_task_by_response_time().get(select_task).map_or(0, |job| job.response_time())
    }

    #[inline]
    /// extract computation time spent in each task and abb
    /// task_name -> (abb_addr -> (interval_count, exec_count, exec_time, woet))
    fn select_abb_profile(
        &self,
        select_task: Option<String>,
    ) -> HashMap<Cow<'static, str>, HashMap<u32, (usize, usize, u64, u64)>> {
        if let Some(select_task) = select_task.as_ref() {
            // Task selected, only profile this task
            let wjptybrt = self.worst_jobs_per_task_by_response_time();
            if let Some(worst_instance) = wjptybrt.get(select_task)
            {
                let t: Vec<_> = self
                    .intervals()
                    .iter()
                    .filter(|x| {
                        x.start_tick < worst_instance.response && x.end_tick > worst_instance.release
                    })
                    .cloned()
                    .collect();
                abb_profile(t)
            } else {
                HashMap::new()
            }
        } else {
            // Profile all tasks
            abb_profile(self.intervals().clone())
        }
    }

    fn need_to_debug(&self) -> bool;
}


pub trait TaskControlBlock: Serialize + for<'a> Deserialize<'a> + Default + Debug + Hash + PartialEq + Clone + SerdeAny {
    fn task_name(&self) -> &String;
    fn task_name_mut(&mut self) -> &mut String;
    // Define methods common to TCBs across different systems
}

//============================= 

/// A trait for looking up data in a QEMU emulation environment.
pub trait QemuLookup {
    fn lookup(emu: &Qemu, addr: ::std::os::raw::c_uint) -> Self;
}

#[macro_export]
macro_rules! impl_emu_lookup {
    ($struct_name:ident) => {
        impl $crate::systemstate::target_os::QemuLookup for $struct_name {
            fn lookup(emu: &Qemu, addr: ::std::os::raw::c_uint) -> $struct_name {
                let mut tmp : [u8; std::mem::size_of::<$struct_name>()] = [0u8; std::mem::size_of::<$struct_name>()];
                unsafe {
                    emu.read_mem(addr.into(), &mut tmp).unwrap();
                    std::mem::transmute::<[u8; std::mem::size_of::<$struct_name>()], $struct_name>(tmp)
                }
            }
        }
    };
}

pub fn compute_hash<T>(obj: &T) -> u64
where
    T: Hash,
{
    let mut s = DefaultHasher::new();
    obj.hash(&mut s);
    s.finish()
}
