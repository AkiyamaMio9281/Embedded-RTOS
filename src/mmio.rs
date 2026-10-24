//! Minimal wrapper for memory-mapped registers

use core::ptr::{read_volatile, write_volatile};

/// A 32-bit peripheral register.
///
/// Creating one is `unsafe` (you vouch that the address really is a register);
/// reading and writing it afterwards is safe — the unsafety is confined to the
/// few dozen constants in `regs.rs`.
#[derive(Clone, Copy)]
pub struct Reg(usize);

impl Reg {
    /// # Safety
    /// `addr` must be a valid, 4-byte-aligned 32-bit register address.
    pub const unsafe fn new(addr: usize) -> Self {
        Reg(addr)
    }

    #[inline(always)]
    pub fn read(self) -> u32 {
        unsafe { read_volatile(self.0 as *const u32) }
    }

    #[inline(always)]
    pub fn write(self, value: u32) {
        unsafe { write_volatile(self.0 as *mut u32, value) }
    }

    #[inline(always)]
    pub fn modify(self, f: impl FnOnce(u32) -> u32) {
        self.write(f(self.read()));
    }
}
