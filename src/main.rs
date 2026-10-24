//! Demo application
//!
//!   blink  (priority 1): toggles LD2 every BLINK_PERIOD_MS
//!   stats  (priority 1): prints a heartbeat every 2 s (same priority as blink, shows round-robin)
//!   button (priority 2): waits on the button semaphore (posted from the EXTI interrupt) and
//!                        cycles the blink speed; after 10 s without a press it prints a
//!                        timeout message (shows a wait with timeout)
//!
//! Serial: ST-LINK virtual COM port, 115200 8N1

#![no_std]
#![no_main]

mod bsp;
mod mmio;
mod os;

use core::fmt::Write;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use bsp::board;
use bsp::uart::{self, Uart};
use os::{Mutex, Semaphore, Stack};

static BLINK_STACK: Stack<256> = Stack::new();
static STATS_STACK: Stack<512> = Stack::new(); // core::fmt needs a fair amount of stack, so give it more
static BUTTON_STACK: Stack<512> = Stack::new();

static CONSOLE: Mutex<Uart> = Mutex::new(Uart); // the UART is behind a lock; you must hold it to print
static BUTTON: Semaphore = Semaphore::new(0);   // interrupt -> task event notification
static BLINK_PERIOD_MS: AtomicU32 = AtomicU32::new(500);

/// Print one line while holding the lock. The guard is dropped at the end of the statement, unlocking it.
macro_rules! log {
    ($($arg:tt)*) => {{
        let mut console = CONSOLE.lock();
        let _ = writeln!(console, $($arg)*);
    }};
}

fn blink() -> ! {
    loop {
        board::led_toggle();
        os::delay(os::ms(BLINK_PERIOD_MS.load(Relaxed)));
    }
}

fn stats() -> ! {
    loop {
        os::delay(os::ms(2000));
        log!("[{}] stats: alive, blink period = {} ms", os::ticks(), BLINK_PERIOD_MS.load(Relaxed));
    }
}

fn button() -> ! {
    const PERIODS: [u32; 4] = [500, 200, 100, 1000];
    let mut idx = 0;

    loop {
        if !BUTTON.wait_timeout(os::ms(10_000)) {
            log!("[{}] button: no press for 10 s (timeout), try the blue B1 button", os::ticks());
            continue;
        }

        os::delay(os::ms(30)); // debounce: only counts if still pressed 30 ms later
        if board::button_is_pressed() {
            idx = (idx + 1) % PERIODS.len();
            BLINK_PERIOD_MS.store(PERIODS[idx], Relaxed);
            log!("[{}] button: blink period -> {} ms", os::ticks(), PERIODS[idx]);
        }

        while BUTTON.try_wait() {} // drop the extra posts caused by contact bounce
    }
}

fn main() -> ! {
    board::init();
    uart::init(115_200);
    let _ = writeln!(Uart, "\n=== mini RTOS (Rust) on NUCLEO-F446RE ===");

    board::set_button_handler(|| BUTTON.post()); // runs inside the EXTI interrupt

    os::spawn(blink, &BLINK_STACK, 1, "blink").unwrap();
    os::spawn(stats, &STATS_STACK, 1, "stats").unwrap();
    os::spawn(button, &BUTTON_STACK, 2, "button").unwrap();

    os::start()
}

/// Every fatal error (stack overflow, HardFault, failed unwrap, ...) ends up here:
/// print the reason and blink the LED fast
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    os::disable_interrupts();
    let _ = writeln!(Uart, "\n*** PANIC: {info}");
    loop {
        board::led_toggle();
        for _ in 0..200_000 {
            unsafe { core::arch::asm!("nop") };
        }
    }
}
