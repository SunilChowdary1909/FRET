use hashbrown::HashMap;
use libafl_qemu::{elf::EasyElf, GuestAddr};
use std::env;

use crate::systemstate::helpers::{load_symbol, try_load_symbol};

pub fn get_target_symbols(elf: &EasyElf) -> HashMap<&'static str, GuestAddr> {
    let mut addrs = HashMap::new();

    addrs.insert(
        "__APP_CODE_START__",
        load_symbol(&elf, "__APP_CODE_START__", false),
    );
    addrs.insert(
        "__APP_CODE_END__",
        load_symbol(&elf, "__APP_CODE_END__", false),
    );
    addrs.insert(
        "__API_CODE_START__",
        load_symbol(&elf, "__API_CODE_START__", false),
    );
    addrs.insert(
        "__API_CODE_END__",
        load_symbol(&elf, "__API_CODE_END__", false),
    );
    addrs.insert(
        "trigger_job_done",
        load_symbol(&elf, "trigger_job_done", false),
    );

    #[cfg(feature = "freertos")]
    crate::systemstate::target_os::freertos::config::add_target_symbols(elf, &mut addrs);
    
    #[cfg(feature = "osek")]
    crate::systemstate::target_os::osek::config::add_target_symbols(elf, &mut addrs);

    // the main address where the fuzzer starts
    // if this is set for freeRTOS it has an influence on where the data will have to be written,
    // since the startup routine copies the data segemnt to it's virtual address
    let main_addr = elf.resolve_symbol(
        &env::var("FUZZ_MAIN").unwrap_or_else(|_| "FUZZ_MAIN".to_owned()),
        0,
    );
    if let Some(main_addr) = main_addr {
        addrs.insert("FUZZ_MAIN", main_addr);
    }

    let input_addr = load_symbol(
        &elf,
        &env::var("FUZZ_INPUT").unwrap_or_else(|_| "FUZZ_INPUT".to_owned()),
        true,
    );
    addrs.insert("FUZZ_INPUT", input_addr);

    let input_length_ptr = try_load_symbol(
        &elf,
        &env::var("FUZZ_LENGTH").unwrap_or_else(|_| "FUZZ_LENGTH".to_owned()),
        true,
    );
    if let Some(input_length_ptr) = input_length_ptr {
        addrs.insert("FUZZ_LENGTH", input_length_ptr);
    }
    let input_counter_ptr = try_load_symbol(
        &elf,
        &env::var("FUZZ_POINTER").unwrap_or_else(|_| "FUZZ_POINTER".to_owned()),
        true,
    );
    if let Some(input_counter_ptr) = input_counter_ptr {
        addrs.insert("FUZZ_POINTER", input_counter_ptr);
    }
    addrs.insert(
        "BREAKPOINT",
        elf.resolve_symbol(
            &env::var("BREAKPOINT").unwrap_or_else(|_| "BREAKPOINT".to_owned()),
            0,
        )
        .expect("Symbol or env BREAKPOINT not found"),
    );

    addrs
}

pub fn get_target_ranges(
    _elf: &EasyElf,
    symbols: &HashMap<&'static str, GuestAddr>,
) -> HashMap<&'static str, std::ops::Range<GuestAddr>> {
    let mut ranges = HashMap::new();

    ranges.insert(
        "APP_CODE",
        symbols["__APP_CODE_START__"]..symbols["__APP_CODE_END__"],
    );
    ranges.insert(
        "API_CODE",
        symbols["__API_CODE_START__"]..symbols["__API_CODE_END__"],
    );

    ranges
}
