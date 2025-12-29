use hashbrown::HashMap;
use libafl_qemu::{elf::EasyElf, GuestAddr};

use crate::{
    fuzzer::get_all_fn_symbol_ranges,
    systemstate::{helpers::{get_function_range, load_symbol}, target_os::freertos::ISR_SYMBOLS},
};

// Add os-specific symbols to the target symbol hashmap
pub fn add_target_symbols(elf: &EasyElf, addrs: &mut HashMap<&'static str, GuestAddr>) {
    // required for system state observation
    addrs.insert("pxCurrentTCB", load_symbol(&elf, "pxCurrentTCB", false)); // loads to the address specified in elf, without respecting program headers
    addrs.insert(
        "pxReadyTasksLists",
        load_symbol(&elf, "pxReadyTasksLists", false),
    );
    addrs.insert(
        "pxDelayedTaskList",
        load_symbol(&elf, "pxDelayedTaskList", false),
    );
    addrs.insert(
        "pxOverflowDelayedTaskList",
        load_symbol(&elf, "pxOverflowDelayedTaskList", false),
    );
    addrs.insert(
        "uxSchedulerSuspended",
        load_symbol(&elf, "uxSchedulerSuspended", false),
    );
    addrs.insert(
        "xSchedulerRunning",
        load_symbol(&elf, "xSchedulerRunning", false),
    );
    addrs.insert(
        "uxCriticalNesting",
        load_symbol(&elf, "uxCriticalNesting", false),
    );
}


// Group functions into api, app and isr functions
pub fn get_range_groups(
    elf: &EasyElf,
    _addrs: &HashMap<&'static str, GuestAddr>,
    ranges: &HashMap<&'static str, std::ops::Range<GuestAddr>>,
) -> HashMap<&'static str, hashbrown::HashMap<String, std::ops::Range<u32>>> {
    let api_range = ranges.get("API_CODE").unwrap();
    let app_range = ranges.get("APP_CODE").unwrap();

    let mut api_fn_ranges = get_all_fn_symbol_ranges(&elf, api_range.clone());
    let mut app_fn_ranges = get_all_fn_symbol_ranges(&elf, app_range.clone());

    // Regular ISR functions, remove from API functions
    let mut isr_fn_ranges: HashMap<String, std::ops::Range<GuestAddr>> = ISR_SYMBOLS
        .iter()
        .filter_map(|x| {
            api_fn_ranges
                .remove(&x.to_string())
                .map(|y| (x.to_string(), y.clone()))
        })
        .collect();
    // User-defined ISR functions, remove from APP functions
    ISR_SYMBOLS.iter().for_each(|x| {
        let _ = (app_fn_ranges
            .remove(&x.to_string())
            .map(|y| (x.to_string(), y.clone())))
        .map(|z| isr_fn_ranges.insert(z.0, z.1));
    });

    // Add the rest of the ISR function, if not already found
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
    return groups;
}
