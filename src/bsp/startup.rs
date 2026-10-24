//! Interrupt vector table + reset handler

use core::arch::asm;

use super::regs::{EXTI15_10_IRQN, NUM_IRQS, SCB_CPACR};
use super::board;
use crate::os;

type Vector = Option<unsafe extern "C" fn()>;

/// The table starts at entry 1 (Reset); entry 0 (initial MSP) is written by link.x.
/// Index = exception number - 1; peripheral IRQ n is at index 15 + n.
#[unsafe(link_section = ".isr_vector")]
#[used]
static VECTOR_TABLE: [Vector; 15 + NUM_IRQS] = build_vector_table();

const fn h(f: unsafe extern "C" fn()) -> Vector {
    Some(f)
}

const fn build_vector_table() -> [Vector; 15 + NUM_IRQS] {
    let mut t = [h(default_handler); 15 + NUM_IRQS];
    t[0] = h(Reset_Handler);
    t[2] = h(hard_fault_handler);
    t[6] = None; // 7..=10 reserved
    t[7] = None;
    t[8] = None;
    t[9] = None;
    t[10] = h(os::port::SVC_Handler);
    t[12] = None; // 13 reserved
    t[13] = h(os::port::PendSV_Handler);
    t[14] = h(os::systick_handler);
    t[15 + EXTI15_10_IRQN] = h(board::exti15_10_handler);
    t
}

#[allow(non_upper_case_globals)]
unsafe extern "C" {
    static _sidata: u32;
    static mut _sdata: u32;
    static mut _edata: u32;
    static mut _sbss: u32;
    static mut _ebss: u32;
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Reset_Handler() {
    // Enable the FPU first (full access to CP10/CP11): the target is eabihf, so the
    // compiler may use floating-point registers at any time
    SCB_CPACR.modify(|v| v | (0xF << 20));
    unsafe { asm!("dsb", "isb", options(nostack, preserves_flags)) };

    // Copy .data from Flash to RAM and zero .bss. No static may be touched before this.
    unsafe {
        let mut src = &raw const _sidata;
        let mut dst = &raw mut _sdata;
        while dst < &raw mut _edata {
            dst.write_volatile(src.read_volatile());
            dst = dst.add(1);
            src = src.add(1);
        }
        let mut dst = &raw mut _sbss;
        while dst < &raw mut _ebss {
            dst.write_volatile(0);
            dst = dst.add(1);
        }
    }

    crate::main()
}

extern "C" fn hard_fault_handler() {
    panic!("HardFault");
}

extern "C" fn default_handler() {
    // Getting here means an interrupt fired that has no handler; check IPSR in the debugger
    loop {}
}
