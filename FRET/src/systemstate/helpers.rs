use hashbrown::HashMap;
use libafl_bolts::prelude::{SerdeAny, SerdeAnyMap};
use libafl_qemu::{elf::EasyElf, read_user_reg_unchecked, GuestAddr, GuestPhysAddr};
use std::{borrow::Cow, cmp::min, hash::{DefaultHasher, Hash, Hasher}, ops::Range};

use crate::{
    fuzzer::{DO_NUM_INTERRUPT, FIRST_INT},
    time::clock::QEMU_ISNS_PER_USEC,
};

use super::ExecInterval;

//============================= API symbols

/// Resolves a virtual address to a physical address using ELF program headers.
/// 
/// # Arguments
/// * `vaddr` - The virtual address to resolve.
/// * `tab` - The ELF file containing program headers.
/// 
/// # Returns
/// The corresponding physical address, or the original address if not found.
fn virt2phys(vaddr: GuestPhysAddr, tab: &EasyElf) -> GuestPhysAddr {
    let ret;
    for i in &tab.goblin().program_headers {
        if i.vm_range().contains(&vaddr.try_into().unwrap()) {
            ret = vaddr - TryInto::<GuestPhysAddr>::try_into(i.p_vaddr).unwrap()
                + TryInto::<GuestPhysAddr>::try_into(i.p_paddr).unwrap();
            return ret - (ret % 2);
        }
    }
    return vaddr;
}

/// Looks up a symbol in the ELF file and returns its address, optionally translating to a physical address.
/// 
/// # Arguments
/// * `elf` - The ELF file to search.
/// * `symbol` - The symbol name to look up.
/// * `do_translation` - Whether to translate the address to a physical address.
/// 
/// # Panics
/// Panics if the symbol is not found.
/// 
/// # Returns
/// The address of the symbol.
pub fn load_symbol(elf: &EasyElf, symbol: &str, do_translation: bool) -> GuestAddr {
    try_load_symbol(elf, symbol, do_translation).expect(&format!("Symbol {} not found", symbol))
}

/// Looks up a symbol in the ELF file and returns its address, optionally translating to a physical address.
/// 
/// # Arguments
/// * `elf` - The ELF file to search.
/// * `symbol` - The symbol name to look up.
/// * `do_translation` - Whether to translate the address to a physical address.
/// 
/// # Returns
/// Some(address) if found, None otherwise.
pub fn try_load_symbol(elf: &EasyElf, symbol: &str, do_translation: bool) -> Option<GuestAddr> {
    let ret = elf.resolve_symbol(symbol, 0);
    if do_translation {
        Option::map_or(ret, None, |x| {
            Some(virt2phys(x as GuestPhysAddr, &elf) as GuestAddr)
        })
    } else {
        ret
    }
}

/// Returns the address range of a function symbol in the ELF file.
/// 
/// # Arguments
/// * `elf` - The ELF file to search.
/// * `symbol` - The function symbol name.
/// 
/// # Returns
/// Some(range) if found, None otherwise.
pub fn get_function_range(elf: &EasyElf, symbol: &str) -> Option<std::ops::Range<GuestAddr>> {
    let gob = elf.goblin();

    let mut funcs: Vec<_> = gob.syms.iter().filter(|x| x.is_function()).collect();
    funcs.sort_unstable_by(|x, y| x.st_value.cmp(&y.st_value));

    for sym in &gob.syms {
        if let Some(sym_name) = gob.strtab.get_at(sym.st_name) {
            if sym_name == symbol {
                if sym.st_value == 0 {
                    return None;
                } else {
                    //#[cfg(cpu_target = "arm")]
                    // Required because of arm interworking addresses aka bit(0) for thumb mode
                    let addr = (sym.st_value as GuestAddr) & !(0x1 as GuestAddr);
                    //#[cfg(not(cpu_target = "arm"))]
                    //let addr = sym.st_value as GuestAddr;
                    // look for first function after addr
                    let sym_end = funcs.iter().find(|x| x.st_value > sym.st_value);
                    if let Some(sym_end) = sym_end {
                        // println!("{} {:#x}..{} {:#x}", gob.strtab.get_at(sym.st_name).unwrap_or(""),addr, gob.strtab.get_at(sym_end.st_name).unwrap_or(""),sym_end.st_value & !0x1);
                        return Some(addr..((sym_end.st_value & !0x1) as GuestAddr));
                    }
                    return None;
                };
            }
        }
    }
    return None;
}

/// Checks if an address is within any of the provided ranges.
/// 
/// # Arguments
/// * `ranges` - A vector of (name, range) tuples.
/// * `addr` - The address to check.
/// 
/// # Returns
/// Some(range) if the address is in any range, None otherwise.
pub fn in_any_range<'a>(
    ranges: &'a Vec<(Cow<'static, str>, Range<u32>)>,
    addr: GuestAddr,
) -> Option<&'a std::ops::Range<GuestAddr>> {
    for (_, r) in ranges {
        if r.contains(&addr) {
            return Some(r);
        }
    }
    return None;
}

//============================= QEMU related utility functions

/// Retrieves the current QEMU instruction count.
/// 
/// # Arguments
/// * `emulator` - The QEMU emulator instance.
/// 
/// # Returns
/// The current instruction count as a u64.
pub fn get_icount(emulator: &libafl_qemu::Qemu) -> u64 {
    unsafe {
        // TODO: investigate why can_do_io is not set sometimes, as this is just a workaround
        let c = emulator.cpu_from_index(0);
        let can_do_io = (*c.raw_ptr()).neg.can_do_io;
        (*c.raw_ptr()).neg.can_do_io = true;
        let r = libafl_qemu::sys::icount_get_raw();
        (*c.raw_ptr()).neg.can_do_io = can_do_io;
        r
    }
}

/// Converts input bytes to a vector of interrupt times, enforcing minimum inter-arrival time.
/// 
/// # Arguments
/// * `buf` - The input byte buffer.
/// * `config` - Tuple of (number of interrupts, minimum inter-arrival time).
/// 
/// # Returns
/// A sorted vector of interrupt times.
pub fn input_bytes_to_interrupt_times(buf: &[u8], config: (usize, u32)) -> Vec<u32> {
    let len = buf.len();
    let mut start_tick;
    let mut ret = Vec::with_capacity(min(DO_NUM_INTERRUPT, len / 4));
    for i in 0..DO_NUM_INTERRUPT {
        let mut buf4b: [u8; 4] = [0, 0, 0, 0];
        if len >= (i + 1) * 4 {
            for j in 0usize..4usize {
                buf4b[j] = buf[i * 4 + j];
            }
            start_tick = u32::from_le_bytes(buf4b);
            if start_tick < FIRST_INT {
                start_tick = 0;
            }
            ret.push(start_tick);
        } else {
            break;
        }
    }
    ret.sort_unstable();
    // obey the minimum inter arrival time while maintaining the sort
    for i in 0..ret.len() {
        if ret[i] == 0 {
            continue;
        }
        for j in i + 1..ret.len() {
            if ret[j] - ret[i] < (config.1 as f32 * QEMU_ISNS_PER_USEC) as u32 {
                // ret[j] = u32::saturating_add(ret[i],config.1 * QEMU_ISNS_PER_USEC);
                ret[j] = 0; // remove the interrupt
                ret.sort_unstable();
                break;
            } else {
                break;
            }
        }
    }
    ret
}

/// Converts interrupt times back to input bytes.
/// 
/// # Arguments
/// * `interrupt_times` - A slice of interrupt times.
/// 
/// # Returns
/// A vector of bytes representing the interrupt times.
pub fn interrupt_times_to_input_bytes(interrupt_times: &[u32]) -> Vec<u8> {
    let mut ret = Vec::with_capacity(interrupt_times.len() * 4);
    for i in interrupt_times {
        ret.extend(u32::to_le_bytes(*i));
    }
    ret
}

/// Reads the return address from the stack frame, handling ARM exception return conventions.
/// 
/// # Arguments
/// * `emu` - The QEMU emulator instance.
/// * `lr` - The link register value.
/// 
/// # Returns
/// The return address from the stack frame.
pub fn read_rec_return_stackframe(emu: &libafl_qemu::Qemu, lr: GuestAddr) -> GuestAddr {
    let lr_ = lr & u32::MAX - 1;
    if lr_ == 0xfffffffc || lr_ == 0xFFFFFFF8 || lr_ == 0xFFFFFFF0 {
        // if 0xFFFFFFF0/1 0xFFFFFFF8/9 -> "main stack" MSP
        let mut buf = [0u8; 4];
        let sp: GuestAddr = if lr_ == 0xfffffffc || lr_ == 0xFFFFFFF0 {
            // PSP
            read_user_reg_unchecked(emu) as u32
        } else {
            emu.read_reg(13).unwrap()
        };
        let ret_pc = sp + 0x18; // https://developer.arm.com/documentation/dui0552/a/the-cortex-m3-processor/exception-model/exception-entry-and-return
        emu.read_mem(ret_pc, buf.as_mut_slice())
            .expect("Failed to read return address");
        return u32::from_le_bytes(buf);
        // elseif 0xfffffffc/d
    } else {
        return lr;
    };
}

//============================= Tracing related utility functions

/// Inserts or updates metadata in a map, returning a mutable reference.
/// 
/// # Arguments
/// * `metadata` - The metadata map.
/// * `default` - Function to create a default value if not present.
/// * `update` - Function to update the value if present.
/// 
/// # Returns
/// A mutable reference to the metadata value.
pub fn metadata_insert_or_update_get<T>(
    metadata: &mut SerdeAnyMap,
    default: impl FnOnce() -> T,
    update: impl FnOnce(&mut T),
) -> &mut T
where
    T: SerdeAny,
{
    if metadata.contains::<T>() {
        let v = metadata.get_mut::<T>().unwrap();
        update(v);
        return v;
    } else {
        return metadata.get_or_insert_with(default);
    }
}

/// Builds an ABB (atomic basic block) profile from execution intervals.
/// 
/// # Arguments
/// * `intervals` - A vector of execution intervals.
/// 
/// # Returns
/// A mapping from task name to ABB address to (interval count, exec count, exec time, woet).
#[allow(unused)]
pub fn abb_profile(
    mut intervals: Vec<ExecInterval>,
) -> HashMap<Cow<'static, str>, HashMap<u32, (usize, usize, u64, u64)>> {
    let mut ret: HashMap<Cow<'static, str>, HashMap<u32, (usize, usize, u64, u64)>> = HashMap::new();
    intervals.sort_by_key(|x| x.get_task_name_unchecked());
    intervals
        .chunk_by_mut(|x, y| x.get_task_name_unchecked() == y.get_task_name_unchecked())
        // Iterate over all tasks
        .for_each(|intv_of_task| {
            // Iterate over all intervals of this task
            intv_of_task.sort_by_key(|y| y.abb.as_ref().unwrap().start);
            // Iterate over each abb of this task
            let mut inter_per_abb_of_task: Vec<&mut [ExecInterval]> = intv_of_task
                .chunk_by_mut(|y, z| y.abb.as_ref().unwrap().start == z.abb.as_ref().unwrap().start)
                .collect();
            // arrange the abbs by their start address
            inter_per_abb_of_task
                .iter_mut()
                .for_each(|ivs_of_abb_of_task| {
                    ivs_of_abb_of_task.sort_by_key(|y| y.abb.as_ref().unwrap().instance_id)
                });
            // find the woet for this abb
            let abb_woet: HashMap<GuestAddr, u64> = inter_per_abb_of_task
                .iter()
                .map(|ivs_of_abb_of_task| {
                    // group intervals by id, sum up the exec time of the abb instance
                    ivs_of_abb_of_task
                        .chunk_by(
                            |y, z| {
                                y.abb.as_ref().unwrap().instance_id
                                    == z.abb.as_ref().unwrap().instance_id
                            },
                        )
                        .map(|intv_of_abb_with_id| {
                            (
                                intv_of_abb_with_id[0].abb.as_ref().unwrap().start,
                                intv_of_abb_with_id
                                    .iter()
                                    .map(|z| z.get_exec_time())
                                    .sum::<_>(),
                            )
                        })
                        .max_by_key(|x| x.1)
                        .unwrap()
                })
                .collect();
            inter_per_abb_of_task.into_iter().for_each(|y| {
                match ret.get_mut(&y[0].get_task_name_unchecked()) {
                    Option::None => {
                        ret.insert(
                            y[0].get_task_name_unchecked(),
                            HashMap::from([(
                                y[0].abb.as_ref().unwrap().start,
                                (
                                    y.len(),
                                    y.iter().filter(|x| x.is_abb_end()).count(),
                                    y.iter().map(|z| z.get_exec_time()).sum::<_>(),
                                    abb_woet[&y[0].abb.as_ref().unwrap().start],
                                ),
                            )]),
                        );
                    }
                    Some(x) => {
                        x.insert(
                            y[0].abb.as_ref().unwrap().start,
                            (
                                y.len(),
                                y.iter().filter(|x| x.is_abb_end()).count(),
                                y.iter().map(|z| z.get_exec_time()).sum(),
                                abb_woet[&y[0].abb.as_ref().unwrap().start],
                            ),
                        );
                    }
                }
            });
        });
    ret
}

/// Returns an immutable reference from a mutable one.
/// 
/// # Arguments
/// * `x` - A mutable reference.
/// 
/// # Returns
/// An immutable reference to the same value.
pub fn unmut<T>(x: &mut T) -> &T {
    &(*x)
}
