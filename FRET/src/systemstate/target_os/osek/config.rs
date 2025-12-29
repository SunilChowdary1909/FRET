/*
 * OSEK/RTA_OS Configuration for FRET Fuzzer
 * Symbol resolution and memory layout
 * Target: AURIX TC4x (TriCore)
 * 
 * Matches symbols from osek.h:
 *   Os_TaskDyn[], Os_ResourceDyn[], Os_AlarmDyn[], Os_CounterDyn[]
 *   Os_TickCounter
 */

use hashbrown::HashMap;
use libafl_qemu::{elf::EasyElf, GuestAddr};

use crate::{
    fuzzer::get_all_fn_symbol_ranges,
    systemstate::helpers::{get_function_range, load_symbol},
};

use super::ISR_SYMBOLS;

/// Add OSEK/RTA_OS specific symbols to the target symbol hashmap
/// These match the globals in osek.h
pub fn add_target_symbols(elf: &EasyElf, addrs: &mut HashMap<&'static str, GuestAddr>) {
    // Task management - dynamic state array
    addrs.insert("Os_TaskDyn", load_symbol(&elf, "Os_TaskDyn", false));
    addrs.insert("Os_TaskCount", load_symbol(&elf, "Os_TaskCount", false));
    addrs.insert("Os_CurrentTask", load_symbol(&elf, "Os_CurrentTask", false));
    
    // Resource management
    addrs.insert("Os_ResourceDyn", load_symbol(&elf, "Os_ResourceDyn", false));
    addrs.insert("Os_ResourceCount", load_symbol(&elf, "Os_ResourceCount", false));
    
    // Alarm management
    addrs.insert("Os_AlarmDyn", load_symbol(&elf, "Os_AlarmDyn", false));
    addrs.insert("Os_AlarmCount", load_symbol(&elf, "Os_AlarmCount", false));
    
    // Counter management
    addrs.insert("Os_CounterDyn", load_symbol(&elf, "Os_CounterDyn", false));
    addrs.insert("Os_CounterCount", load_symbol(&elf, "Os_CounterCount", false));
    
    // Timing
    addrs.insert("Os_TickCounter", load_symbol(&elf, "Os_TickCounter", false));
    
    // Ready queue (if used)
    addrs.insert("Os_ReadyQueue", load_symbol(&elf, "Os_ReadyQueue", false));
    
    // Static task configs (application-defined)
    addrs.insert("Os_TaskCfg", load_symbol(&elf, "Os_TaskCfg", false));
}

/// Group functions into API, app, and ISR categories
pub fn get_range_groups(
    elf: &EasyElf,
    _addrs: &HashMap<&'static str, GuestAddr>,
    ranges: &HashMap<&'static str, std::ops::Range<GuestAddr>>,
) -> HashMap<&'static str, HashMap<String, std::ops::Range<GuestAddr>>> {
    let api_range = ranges.get("API_CODE").unwrap();
    let app_range = ranges.get("APP_CODE").unwrap();

    let mut api_fn_ranges = get_all_fn_symbol_ranges(&elf, api_range.clone());
    let mut app_fn_ranges = get_all_fn_symbol_ranges(&elf, app_range.clone());

    // OSEK API functions to identify
    const OSEK_API_SYMBOLS: &[&str] = &[
        "ActivateTask",
        "TerminateTask",
        "ChainTask",
        "Schedule",
        "GetTaskID",
        "GetTaskState",
        "GetResource",
        "ReleaseResource",
        "SetEvent",
        "ClearEvent",
        "GetEvent",
        "WaitEvent",
        "GetAlarmBase",
        "GetAlarm",
        "SetRelAlarm",
        "SetAbsAlarm",
        "CancelAlarm",
        "IncrementCounter",
        "GetCounterValue",
        "StartOS",
        "ShutdownOS",
        "DisableAllInterrupts",
        "EnableAllInterrupts",
        "SuspendAllInterrupts",
        "ResumeAllInterrupts",
        "SuspendOSInterrupts",
        "ResumeOSInterrupts",
    ];

    // Ensure OSEK API functions are in api_fn_ranges
    for api_fn in OSEK_API_SYMBOLS {
        if api_fn_ranges.get(&api_fn.to_string()).is_none() {
            if let Some(fr) = get_function_range(&elf, api_fn) {
                api_fn_ranges.insert(api_fn.to_string(), fr);
            }
        }
    }

    // ISR functions - remove from API/APP and collect separately
    let mut isr_fn_ranges: HashMap<String, std::ops::Range<GuestAddr>> = ISR_SYMBOLS
        .iter()
        .filter_map(|x| {
            api_fn_ranges
                .remove(&x.to_string())
                .map(|y| (x.to_string(), y.clone()))
        })
        .collect();
    
    // Also check APP functions for user-defined ISRs
    ISR_SYMBOLS.iter().for_each(|x| {
        let _ = app_fn_ranges
            .remove(&x.to_string())
            .map(|y| isr_fn_ranges.insert(x.to_string(), y));
    });

    // Add ISRs not yet found
    for i in ISR_SYMBOLS {
        if isr_fn_ranges.get(&i.to_string()).is_none() {
            if let Some(fr) = get_function_range(&elf, i) {
                isr_fn_ranges.insert(i.to_string(), fr);
            }
        }
    }

    let mut groups = HashMap::new();
    groups.insert("API_FN", api_fn_ranges);
    groups.insert("APP_FN", app_fn_ranges);
    groups.insert("ISR_FN", isr_fn_ranges);
    
    groups
}
