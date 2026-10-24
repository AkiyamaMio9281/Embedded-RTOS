//! Mini RTOS kernel: task table, scheduler, tick, delays
//!
//! Scheduling rules:
//!   - Preemptive priority scheduling; a higher number means higher priority.
//!     The idle task has priority 0, so user tasks should use >= 1.
//!   - Tasks of equal priority are round-robined once per tick.
//!   - A task gives up the CPU when it blocks (delay / waiting on a semaphore).

mod config;
pub mod port;
mod sync;

pub use sync::{Mutex, Semaphore};

use core::cell::UnsafeCell;
use core::ptr::{null, null_mut};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use config::*;

/// Pass to `delay` / `Semaphore::wait_timeout` to wait forever
pub const FOREVER: u32 = u32::MAX;

/// Milliseconds -> ticks
pub const fn ms(ms: u32) -> u32 {
    ms * TICK_HZ / 1000
}

// ---------------------------------------------------------------------------
// Task control block & global kernel state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Unused,
    Ready,
    Blocked,
}

#[repr(C)]
struct Tcb {
    sp: *mut u32,                  // must be the first field: PendSV asm reads/writes it at offset 0
    stack_base: *mut u32,          // lowest address of the stack, holds the canary
    name: &'static str,
    prio: u8,
    state: State,
    waiting_on: *const Semaphore,  // semaphore being waited on; null for a plain delay
    timed_wait: bool,              // whether wake_tick is valid
    timed_out: bool,               // whether the semaphore wait ended by timeout
    wake_tick: u32,
}

impl Tcb {
    const EMPTY: Tcb = Tcb {
        sp: null_mut(),
        stack_base: null_mut(),
        name: "",
        prio: 0,
        state: State::Unused,
        waiting_on: null(),
        timed_wait: false,
        timed_out: false,
        wake_tick: 0,
    };
}

struct Kernel {
    tasks: [Tcb; MAX_TASKS],
    count: usize,
    current: usize,
    ticks: u32,
}

struct KernelCell(UnsafeCell<Kernel>);

// SAFETY: only accessed through with_kernel with interrupts disabled; no concurrency on a single core
unsafe impl Sync for KernelCell {}

static KERNEL: KernelCell = KernelCell(UnsafeCell::new(Kernel {
    tasks: [Tcb::EMPTY; MAX_TASKS],
    count: 0,
    current: 0,
    ticks: 0,
}));

// These two are accessed by symbol from the asm in port.rs
/// TCB pointer of the current task (AtomicPtr has the same layout as a raw pointer, so asm can ldr it directly)
static OS_CURRENT: AtomicPtr<Tcb> = AtomicPtr::new(null_mut());
/// Set to 1 by SVC_Handler; SysTick only starts scheduling after that
static OS_RUNNING: AtomicU32 = AtomicU32::new(0);

fn running() -> bool {
    OS_RUNNING.load(Ordering::Relaxed) != 0
}

/// Access kernel data with interrupts disabled. Single core + interrupts off = we are the only
/// thread of execution right now, so handing out `&mut Kernel` is sound.
/// The closure must not call with_kernel again.
fn with_kernel<R>(f: impl FnOnce(&mut Kernel) -> R) -> R {
    let primask = port::irq_save();
    let r = f(unsafe { &mut *KERNEL.0.get() });
    port::irq_restore(primask);
    r
}

/// Raw pointer to the i-th TCB (for the asm; does not go through a &mut reference)
fn tcb_ptr(i: usize) -> *mut Tcb {
    unsafe { (&raw mut (*KERNEL.0.get()).tasks).cast::<Tcb>().add(i) }
}

impl Kernel {
    /// Scan one full round starting after the current task and pick the highest-priority ready task.
    /// The current task comes last, so on equal priority it yields to the others — that is the round-robin.
    fn pick_next(&self) -> usize {
        let mut best: Option<usize> = None;
        for i in 1..=self.count {
            let idx = (self.current + i) % self.count;
            let t = &self.tasks[idx];
            if t.state == State::Ready && best.is_none_or(|b| t.prio > self.tasks[b].prio) {
                best = Some(idx);
            }
        }
        best.expect("idle task is always ready")
    }

    /// Block the current task and request a switch. The caller is inside with_kernel;
    /// as soon as interrupts are restored PendSV runs and the task is switched out.
    /// It only continues after with_kernel once it has been woken up.
    fn block_current(&mut self, sem: *const Semaphore, ticks: u32) {
        assert!(running() && !port::in_isr(), "blocking call outside of a task");
        let now = self.ticks;
        let t = &mut self.tasks[self.current];
        t.waiting_on = sem;
        t.timed_out = false;
        t.timed_wait = ticks != FOREVER;
        t.wake_tick = now.wrapping_add(ticks);
        t.state = State::Blocked;
        port::pend_switch();
    }
}

/// Called from the PendSV asm: check the stack, pick the next task
extern "C" fn switch_context() {
    with_kernel(|k| {
        let cur = &k.tasks[k.current];
        let canary = unsafe { cur.stack_base.read_volatile() };
        if canary != STACK_CANARY || cur.sp < cur.stack_base {
            panic!("stack overflow in task '{}'", cur.name);
        }
        k.current = k.pick_next();
        OS_CURRENT.store(tcb_ptr(k.current), Ordering::Relaxed);
    });
}

pub extern "C" fn systick_handler() {
    if !running() {
        return;
    }
    // Other interrupts (e.g. the button) may call Semaphore::post and modify the task table, so disable interrupts
    with_kernel(|k| {
        k.ticks = k.ticks.wrapping_add(1);
        let now = k.ticks;
        for t in &mut k.tasks[..k.count] {
            // Compare via signed difference so tick wrap-around (~49 days) is handled correctly
            if t.state == State::Blocked && t.timed_wait && now.wrapping_sub(t.wake_tick) as i32 >= 0 {
                t.timed_out = !t.waiting_on.is_null();
                t.waiting_on = null();
                t.timed_wait = false;
                t.state = State::Ready;
            }
        }
        port::pend_switch(); // reschedule every tick: wake higher-priority tasks + round-robin equal ones
    });
}

// ---------------------------------------------------------------------------
// Task stacks
// ---------------------------------------------------------------------------

/// A task stack, kept in a static: `static S: Stack<512> = Stack::new();`
/// Each Stack can be given to only one task; a second spawn with it returns an error.
#[repr(C, align(8))]
pub struct Stack<const N: usize> {
    mem: UnsafeCell<[u32; N]>,
    taken: AtomicBool,
}

// SAFETY: mem is only used by the one task whose take() succeeded
unsafe impl<const N: usize> Sync for Stack<N> {}

impl<const N: usize> Stack<N> {
    pub const fn new() -> Self {
        Stack { mem: UnsafeCell::new([0; N]), taken: AtomicBool::new(false) }
    }

    fn take(&self) -> Option<*mut u32> {
        if self.taken.swap(true, Ordering::AcqRel) {
            None
        } else {
            Some(self.mem.get().cast())
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum SpawnError {
    TooManyTasks,
    StackInUse,
}

/// Create a task. The task function has type `fn() -> !`: the compiler guarantees it never returns.
pub fn spawn<const N: usize>(
    entry: fn() -> !,
    stack: &'static Stack<N>,
    prio: u8,
    name: &'static str,
) -> Result<(), SpawnError> {
    const { assert!(N >= MIN_STACK_WORDS, "task stack too small") };

    let base = stack.take().ok_or(SpawnError::StackInUse)?;
    with_kernel(|k| {
        if k.count >= MAX_TASKS {
            return Err(SpawnError::TooManyTasks);
        }
        // SAFETY: base..base+N is memory that take() handed to us exclusively
        let sp = unsafe {
            base.write(STACK_CANARY);
            port::init_stack(base.add(N), entry)
        };
        k.tasks[k.count] = Tcb { sp, stack_base: base, name, prio, state: State::Ready, ..Tcb::EMPTY };
        k.count += 1;

        if running() && prio > k.tasks[k.current].prio {
            port::pend_switch();
        }
        Ok(())
    })
}

/// Create the idle task and start the scheduler; never returns
pub fn start() -> ! {
    static IDLE_STACK: Stack<IDLE_STACK_WORDS> = Stack::new();
    spawn(idle, &IDLE_STACK, 0, "idle").expect("no TCB left for idle task");

    with_kernel(|k| {
        k.current = k.count - 1;
        k.current = k.pick_next();
        OS_CURRENT.store(tcb_ptr(k.current), Ordering::Relaxed);
    });
    port::start_first_task()
}

fn idle() -> ! {
    loop {
        // Nothing to do: sleep until the next interrupt
        unsafe { core::arch::asm!("wfi", options(nomem, nostack, preserves_flags)) };
    }
}

/// Delay for a number of ticks. Task context only.
pub fn delay(ticks: u32) {
    if ticks == 0 {
        yield_now();
        return;
    }
    with_kernel(|k| k.block_current(null(), ticks));
}

/// Voluntarily give the CPU to other tasks of the same priority
pub fn yield_now() {
    if running() {
        port::pend_switch();
    }
}

pub fn ticks() -> u32 {
    with_kernel(|k| k.ticks)
}

/// Disable all interrupts (used by the panic handler)
pub fn disable_interrupts() {
    let _ = port::irq_save();
}
