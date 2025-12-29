#pragma once

#include "qemu/osdep.h"
#include "qapi/error.h"

#include "exec/exec-all.h"
#include "exec/tb-flush.h"

#include "libafl/exit.h"
#include "libafl/hook.h"

struct libafl_jmp_hook {
    uint64_t (*gen)(uint64_t data, target_ulong src, target_ulong dst);
    void (*exec)(uint64_t data, target_ulong src, target_ulong dst, uint64_t id);
    uint64_t data;
    size_t num;
    TCGHelperInfo helper_info;
    struct libafl_jmp_hook* next;
};

extern struct libafl_jmp_hook* libafl_jmp_hooks;

size_t libafl_add_jmp_hook(uint64_t (*gen)(uint64_t data, target_ulong src, target_ulong dst),
                          void (*exec)(uint64_t data, target_ulong src, target_ulong dst, uint64_t id),
                          uint64_t data);

void libafl_gen_jmp(target_ulong src, target_ulong dst);

// Use an indirect jump target
void libafl_gen_jmp_dynamic(target_ulong src, TCGv_i32 dst);
int libafl_qemu_remove_jmp_hook(size_t num, int invalidate);