# Mini RTOS — NUCLEO-F446RE (Rust)

A teaching RTOS written from scratch in Rust. **No third-party crates** (no `cortex-m`,
`cortex-m-rt` or HAL): the startup code, register access and context switching are all
visible in the source.

**Features**

- Preemptive priority scheduling (higher number = higher priority), round-robin among equal priorities every tick
- `os::delay`, `os::yield_now`
- `Semaphore`: counting semaphore with optional timeout; `post` can be called from interrupts
- `Mutex<T>`: the lock and its data are bound together, and the guard unlocks automatically when it goes out of scope
- FPU-aware context switch (Cortex-M4F lazy stacking)
- Stack overflow detection (canary at the bottom of each stack); all fatal errors go through `panic!` and print the file and line

## Layout

```
src/main.rs           demo application + panic handler
src/os/mod.rs         kernel: task table, scheduler, tick, delays, Stack<N>
src/os/sync.rs        Semaphore, Mutex
src/os/port.rs        CPU-specific: stack init, PendSV/SVC asm, starting the first task
src/os/config.rs      configuration (clock, tick rate, max tasks)
src/bsp/startup.rs    vector table + Reset_Handler
src/bsp/regs.rs       register addresses used
src/bsp/board.rs      LED / button
src/bsp/uart.rs       UART (implements fmt::Write)
src/mmio.rs           minimal register read/write wrapper
link.x                linker script
```

## 1. Install the tools

- **Rust** (stable) plus the Cortex-M4F target: `rustup target add thumbv7em-none-eabihf`
- **Arm GNU Toolchain**: only `arm-none-eabi-objcopy` (ELF to .bin) and `arm-none-eabi-size` are used.
  `winget install Arm.GnuArmEmbeddedToolchain`

## 2. Build & flash

```powershell
.\build.ps1          # cargo build --release, produces target\rtos.bin
.\build.ps1 -Flash   # build and copy to the board's USB drive (NOD_F446RE); the ST-LINK programs and resets it
```

Without the script, dragging `target\rtos.bin` onto the `NOD_F446RE` drive does the same thing.

> If PowerShell refuses to run the script, run once: `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`
> If LD1 keeps blinking red after plugging in and the PC doesn't react, the USB cable is most likely
> charge-only — use a data cable.

## 3. What you should see

- Green LED LD2 blinks with a 500 ms period
- Open the serial port (*STMicroelectronics STLink Virtual COM Port* in Device Manager, 115200 8N1);
  a `stats: alive ...` line appears every 2 s
- Pressing the blue B1 button cycles the blink period through 500 → 200 → 100 → 1000 ms and prints the new period
- After 10 s without a press, the button task's semaphore wait times out and prints a hint

## 4. How it works

### Scheduling

Each task has a TCB (`src/os/mod.rs`) whose state is Ready or Blocked.
`pick_next()` scans one full round starting after the current task and picks the highest-priority
Ready task. The current task comes last, so on equal priority it yields to the others — that is the
round-robin. The idle task has priority 0, is always ready, and runs `wfi` to save power.

When a switch happens:

| Event | Who requests the switch |
|---|---|
| Every tick (1 ms) | `systick_handler` wakes due tasks, then requests a switch |
| A task blocks in `delay` / `wait` | the task itself |
| `Semaphore::post` wakes a higher-priority task | the caller of post (which may be an interrupt) |

"Requesting a switch" means setting PendSV pending. PendSV is the lowest-priority exception, so it runs
only after all other interrupts are done; the actual register save/restore happens inside it
(`src/os/port.rs`).

All kernel state lives in one `UnsafeCell<Kernel>` and is only reachable through
`with_kernel(|k| ...)`. That function disables interrupts first; on a single core there is then only
one thread of execution, so handing out `&mut Kernel` is sound.

### A task's stack during a context switch

```
low addr  sp -> R4 ... R11, EXC_RETURN    saved manually by PendSV (stmdb)
               [S16 ... S31]             only if the task has used the FPU
               R0 R1 R2 R3 R12 LR PC xPSR  saved by hardware on exception entry
high addr      [S0 ... S15, FPSCR]        FPU frame only, saved by hardware
```

`Tcb::sp` (the first field of a `#[repr(C)]` struct) holds the topmost `sp`. A new task's stack is given
the same kind of frame by `port::init_stack`, with PC pointing at `task_trampoline` and R0 holding the
task function. So the first time it is switched in, it looks as if it had been "interrupted": the
exception return jumps into the trampoline, which then calls the task function.

### Startup

`os::start()` creates the idle task, picks the first task, configures SysTick and executes `svc 0`.
`SVC_Handler` restores the first task's registers from its stack and returns with `bx lr`
(EXC_RETURN = 0xFFFFFFFD) into thread mode on the PSP, and the first task starts running. From then on
interrupts use the MSP and each task uses its own PSP.

### What Rust guarantees

| Construct | Guarantee |
|---|---|
| Task type is `fn() -> !` | a task function cannot return |
| `static S: Stack<256>`, `take()` on `spawn` | a stack can be given to only one task; anything smaller than `MIN_STACK_WORDS` is a **compile-time** error |
| `Semaphore::new` is a `const fn` | it goes straight into a `static`; "forgot to initialize" is impossible |
| `Mutex<Uart>` + guard | the UART can't be touched without the lock, and unlocking can't be forgotten |
| `Reg::new` is `unsafe`, reads/writes are safe | the unsafety is confined to the address constants in `regs.rs` |
| `panic!` + `#[panic_handler]` | stack overflow, HardFault and failed `unwrap` all print their location |

`unsafe` is concentrated in: register address constants, `with_kernel`, stack initialization, the asm,
and the `.data` copy at startup.

## 5. Rules / known limitations

- `delay`, `wait`, `wait_timeout` and `Mutex::lock` are task-only (calling them from an interrupt or before
  `start()` panics); `post` and `try_wait` work from both tasks and interrupts
- Tasks that use `core::fmt` (`writeln!`) should have at least 512 words (2 KB) of stack
- At most `MAX_TASKS` tasks (including idle); tasks can be created but not deleted
- Multiple waiters of equal priority are woken in creation order, not FIFO
- `Mutex` has no priority inheritance, so priority inversion is possible
- `delay` is relative, so periodic tasks drift (each period also includes the time spent printing)
- The CPU runs on the default 16 MHz HSI. Going to 180 MHz requires configuring the PLL and Flash wait
  states, and updating `CPU_HZ` in `os/config.rs` and `PCLK1_HZ` in `bsp/board.rs`

## 6. Ideas for next steps

1. `delay_until`: remove the cumulative drift of periodic tasks
2. Configure the PLL for 180 MHz
3. A message queue `Queue<T, N>`
4. A mutex with priority inheritance
5. Stack usage statistics (fill the whole stack with a pattern and count how much is still untouched)
6. UART receive interrupt + a simple shell (`ps` to show task states)
