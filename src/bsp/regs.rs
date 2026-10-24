//! The minimal set of register definitions used by this project (addresses from RM0390)

use crate::mmio::Reg;

// ---- RCC ----
pub const RCC_AHB1ENR: Reg = unsafe { Reg::new(0x4002_3830) };
pub const RCC_APB1ENR: Reg = unsafe { Reg::new(0x4002_3840) };
pub const RCC_APB2ENR: Reg = unsafe { Reg::new(0x4002_3844) };
pub const RCC_AHB1ENR_GPIOAEN: u32 = 1 << 0;
pub const RCC_AHB1ENR_GPIOCEN: u32 = 1 << 2;
pub const RCC_APB1ENR_USART2EN: u32 = 1 << 17;
pub const RCC_APB2ENR_SYSCFGEN: u32 = 1 << 14;

// ---- GPIO ----
pub struct Gpio(usize);

impl Gpio {
    pub const fn moder(&self) -> Reg { unsafe { Reg::new(self.0) } }
    pub const fn idr(&self) -> Reg { unsafe { Reg::new(self.0 + 0x10) } }
    pub const fn odr(&self) -> Reg { unsafe { Reg::new(self.0 + 0x14) } }
    pub const fn afrl(&self) -> Reg { unsafe { Reg::new(self.0 + 0x20) } }
}

pub const GPIOA: Gpio = Gpio(0x4002_0000);
pub const GPIOC: Gpio = Gpio(0x4002_0800);

// ---- USART2 ----
pub mod usart2 {
    use crate::mmio::Reg;
    pub const SR: Reg = unsafe { Reg::new(0x4000_4400) };
    pub const DR: Reg = unsafe { Reg::new(0x4000_4404) };
    pub const BRR: Reg = unsafe { Reg::new(0x4000_4408) };
    pub const CR1: Reg = unsafe { Reg::new(0x4000_440C) };
}
pub const USART_SR_TXE: u32 = 1 << 7;
pub const USART_CR1_RE: u32 = 1 << 2;
pub const USART_CR1_TE: u32 = 1 << 3;
pub const USART_CR1_UE: u32 = 1 << 13;

// ---- SYSCFG / EXTI ----
pub const SYSCFG_EXTICR4: Reg = unsafe { Reg::new(0x4001_3814) };
pub const EXTI_IMR: Reg = unsafe { Reg::new(0x4001_3C00) };
pub const EXTI_FTSR: Reg = unsafe { Reg::new(0x4001_3C0C) };
pub const EXTI_PR: Reg = unsafe { Reg::new(0x4001_3C14) };

// ---- Cortex-M4 core ----
pub const SCB_CPACR: Reg = unsafe { Reg::new(0xE000_ED88) };

const fn nvic_iser(n: usize) -> Reg { unsafe { Reg::new(0xE000_E100 + 4 * n) } }
const fn nvic_ipr(n: usize) -> Reg { unsafe { Reg::new(0xE000_E400 + 4 * n) } }

pub fn nvic_enable(irq: usize) {
    nvic_iser(irq / 32).write(1 << (irq % 32));
}

pub fn nvic_set_priority(irq: usize, prio: u8) {
    let shift = (irq % 4) * 8;
    nvic_ipr(irq / 4).modify(|v| (v & !(0xFF << shift)) | ((prio as u32) << shift));
}

// ---- IRQ numbers ----
pub const EXTI15_10_IRQN: usize = 40;
pub const NUM_IRQS: usize = 97;
