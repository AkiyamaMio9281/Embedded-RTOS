//! Kernel compile-time configuration

/// Kernel clock. The F446 runs on the 16 MHz HSI after reset; update this if you change the clock tree
pub const CPU_HZ: u32 = 16_000_000;
/// SysTick frequency: 1 tick = 1 ms
pub const TICK_HZ: u32 = 1000;

/// Includes the idle task
pub const MAX_TASKS: usize = 8;
pub const IDLE_STACK_WORDS: usize = 128;
/// Hardware frame (with FPU) + software frame ≈ 51 words, plus some headroom
pub const MIN_STACK_WORDS: usize = 64;

/// Written at the lowest address of every task stack; if it gets overwritten, the stack overflowed
pub const STACK_CANARY: u32 = 0xDEAD_BEEF;
