//! Cortex-M4F context switch
//!
//! When a task is switched out, its stack (PSP) holds, from low to high address:
//!
//! ```text
//!   sp ->  R4 R5 R6 R7 R8 R9 R10 R11 EXC_RETURN    <- saved manually by PendSV (stmdb)
//!         [S16 ... S31]                            <- only if the task has used the FPU
//!          R0 R1 R2 R3 R12 LR PC xPSR              <- pushed by hardware on exception entry
//!         [S0 ... S15 FPSCR reserved]              <- same, FPU frame only
//! ```
//!
//! `Tcb::sp` holds the topmost sp.

use core::arch::{asm, naked_asm};

use super::config::{CPU_HZ, TICK_HZ};
use crate::mmio::Reg;

const SYST_CSR: Reg = unsafe { Reg::new(0xE000_E010) };
const SYST_RVR: Reg = unsafe { Reg::new(0xE000_E014) };
const SYST_CVR: Reg = unsafe { Reg::new(0xE000_E018) };
const SCB_ICSR: Reg = unsafe { Reg::new(0xE000_ED04) };
const SCB_SHPR3: Reg = unsafe { Reg::new(0xE000_ED20) };
const ICSR_PENDSVSET: u32 = 1 << 28;

/// Disable interrupts and return the previous PRIMASK; can be nested
#[inline(always)]
pub fn irq_save() -> u32 {
    let primask: u32;
    unsafe { asm!("mrs {}, primask", "cpsid i", out(reg) primask, options(nostack, preserves_flags)) };
    primask
}

#[inline(always)]
pub fn irq_restore(primask: u32) {
    unsafe { asm!("msr primask, {}", "isb", in(reg) primask, options(nostack, preserves_flags)) };
}

/// Whether we are currently in an interrupt (Handler mode)
#[inline(always)]
pub fn in_isr() -> bool {
    let ipsr: u32;
    unsafe { asm!("mrs {}, ipsr", out(reg) ipsr, options(nomem, nostack, preserves_flags)) };
    ipsr & 0x1FF != 0
}

/// Request a context switch. PendSV has the lowest priority, so the actual switch happens
/// only after all interrupts have been handled and interrupts are enabled again.
#[inline(always)]
pub fn pend_switch() {
    SCB_ICSR.write(ICSR_PENDSVSET);
    unsafe { asm!("dsb", "isb", options(nostack, preserves_flags)) };
}

/// The first time a task is switched in, the exception return "jumps" here with R0 = the task function
extern "C" fn task_trampoline(entry: usize) -> ! {
    // SAFETY: init_stack put a fn() -> ! into R0
    let entry: fn() -> ! = unsafe { core::mem::transmute(entry) };
    entry()
}

/// Fake an "interrupted" context at the top of a new task's stack; returns the sp after saving it
///
/// # Safety
/// `stack_top` must point to the end of a memory block that is large enough (>= MIN_STACK_WORDS)
/// and owned exclusively by this task.
pub unsafe fn init_stack(stack_top: *mut u32, entry: fn() -> !) -> *mut u32 {
    // AAPCS requires the stack to be 8-byte aligned after exception return
    let mut sp = stack_top.map_addr(|a| a & !7);
    let mut push = |v: u32| unsafe {
        sp = sp.sub(1);
        sp.write(v);
    };

    // Hardware frame: popped automatically by the CPU on exception return
    push(0x0100_0000);                           // xPSR: only the Thumb bit set
    push(task_trampoline as *const () as u32 & !1); // PC (Thumb bit cleared)
    push(0);                                     // LR: tasks never return; 0 stops debugger backtraces here
    push(0x1212_1212);                           // R12
    push(0x0303_0303);                           // R3
    push(0x0202_0202);                           // R2
    push(0x0101_0101);                           // R1
    push(entry as *const () as u32);             // R0: argument for task_trampoline

    // Software frame: order must match `ldmia {r4-r11, lr}` in PendSV
    push(0xFFFF_FFFD);                           // EXC_RETURN: thread mode, use PSP, no FPU frame
    for r in (4..=11u32).rev() {
        push(r * 0x0101_0101);                   // R11..R4, recognizable values for debugging
    }
    sp
}

/// Start the first task: reset MSP, enable interrupts, enter an exception via SVC,
/// then "exception-return" into the task
pub fn start_first_task() -> ! {
    // PendSV and SysTick get the lowest priority so a context switch never interrupts another ISR
    SCB_SHPR3.modify(|v| v | (0xFF << 16) | (0xFF << 24));

    SYST_RVR.write(CPU_HZ / TICK_HZ - 1);
    SYST_CVR.write(0);
    SYST_CSR.write(7); // processor clock | interrupt enable | counter enable

    unsafe {
        asm!(
            "movw r0, #0xED08",   // r0 = &SCB->VTOR
            "movt r0, #0xE000",
            "ldr  r0, [r0]",      // r0 = vector table address
            "ldr  r0, [r0]",      // r0 = vector_table[0] = initial MSP
            "msr  msp, r0",       // main()'s stack is no longer needed; the whole MSP goes to interrupts
            "movs r0, #0",        // clear CONTROL.FPCA: discard any FPU context left over from main
            "msr  control, r0",
            "cpsie i",
            "cpsie f",
            "dsb",
            "isb",
            "svc  0",             // -> SVC_Handler, never comes back
            options(noreturn),
        )
    }
}

#[allow(non_snake_case)]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SVC_Handler() {
    naked_asm!(
        "movw  r3, :lower16:{running}",   // OS_RUNNING = 1; SysTick starts scheduling from now on
        "movt  r3, :upper16:{running}",
        "movs  r1, #1",
        "str   r1, [r3]",
        "movw  r3, :lower16:{current}",
        "movt  r3, :upper16:{current}",
        "ldr   r1, [r3]",                 // r1 = OS_CURRENT
        "ldr   r0, [r1]",                 // r0 = OS_CURRENT->sp
        "ldmia r0!, {{r4-r11, lr}}",      // restore R4-R11 and EXC_RETURN
        "msr   psp, r0",
        "isb",
        "bx    lr",                       // exception return: hardware pops R0-R3,R12,LR,PC,xPSR from PSP
        running = sym super::OS_RUNNING,
        current = sym super::OS_CURRENT,
    )
}

#[allow(non_snake_case)]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PendSV_Handler() {
    naked_asm!(
        // ---- save the current task ----
        "mrs      r0, psp",
        "isb",
        "movw     r3, :lower16:{current}",
        "movt     r3, :upper16:{current}",
        "ldr      r2, [r3]",              // r2 = OS_CURRENT
        "tst      lr, #0x10",             // EXC_RETURN bit4 == 0 means the task used the FPU
        "it       eq",
        "vstmdbeq r0!, {{s16-s31}}",
        "stmdb    r0!, {{r4-r11, lr}}",
        "str      r0, [r2]",              // OS_CURRENT->sp = r0

        // ---- pick the next task ----
        "push     {{r0, r3}}",            // push two registers to keep MSP 8-byte aligned
        "cpsid    i",
        "bl       {switch}",
        "cpsie    i",
        "pop      {{r0, r3}}",

        // ---- restore the new task ----
        "ldr      r1, [r3]",              // r1 = OS_CURRENT (updated)
        "ldr      r0, [r1]",              // r0 = OS_CURRENT->sp
        "ldmia    r0!, {{r4-r11, lr}}",
        "tst      lr, #0x10",
        "it       eq",
        "vldmiaeq r0!, {{s16-s31}}",
        "msr      psp, r0",
        "isb",
        "bx       lr",
        current = sym super::OS_CURRENT,
        switch = sym super::switch_context,
    )
}
