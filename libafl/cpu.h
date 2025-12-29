#pragma once

#include "qemu/osdep.h"

#ifndef CONFIG_USER_ONLY
#include "exec/memory.h"
#include "qemu/rcu.h"
#include "cpu.h"
#endif

#ifndef CONFIG_USER_ONLY
uint8_t* libafl_paddr2host(CPUState* cpu, hwaddr addr, bool is_write);
hwaddr libafl_qemu_current_paging_id(CPUState* cpu);
#endif

target_ulong libafl_page_from_addr(target_ulong addr);
CPUState* libafl_qemu_get_cpu(int cpu_index);
int libafl_qemu_num_cpus(void);
CPUState* libafl_qemu_current_cpu(void);
int libafl_qemu_cpu_index(CPUState*);
int libafl_qemu_write_reg(CPUState* cpu, int reg, uint8_t* val);
int libafl_qemu_read_reg(CPUState* cpu, int reg, uint8_t* val);
int libafl_qemu_num_regs(CPUState* cpu);
void libafl_flush_jit(void);
void libafl_breakpoint_invalidate(CPUState* cpu, target_ulong pc);

#ifdef CONFIG_USER_ONLY
int libafl_qemu_main(void);
int libafl_qemu_run(void);
void libafl_set_qemu_env(CPUArchState* env);
#endif

#ifdef TARGET_ARM
int libafl_qemu_read_user_sp_unchecked(CPUState* cpu);
#endif

#ifdef TARGET_TRICORE
// TriCore-specific register access for OSEK/RTA_OS fuzzing
uint32_t libafl_qemu_read_user_sp_unchecked(CPUState* cpu);
uint32_t libafl_qemu_tricore_read_ra(CPUState* cpu);
uint32_t libafl_qemu_tricore_read_pc(CPUState* cpu);
uint32_t libafl_qemu_tricore_read_psw(CPUState* cpu);
uint32_t libafl_qemu_tricore_read_pcxi(CPUState* cpu);
uint32_t libafl_qemu_tricore_read_icr(CPUState* cpu);
uint32_t libafl_qemu_tricore_read_fcx(CPUState* cpu);
uint32_t libafl_qemu_tricore_read_dreg(CPUState* cpu, int reg);
uint32_t libafl_qemu_tricore_read_areg(CPUState* cpu, int reg);
int libafl_qemu_tricore_is_supervisor(CPUState* cpu);
uint8_t libafl_qemu_tricore_get_cpu_priority(CPUState* cpu);
#endif
