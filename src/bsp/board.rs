//! LD2 green LED (PA5), B1 blue button (PC13)

use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, Ordering};

use super::regs::*;

pub const PCLK1_HZ: u32 = 16_000_000; // 16 MHz HSI after reset, APB1 not divided

const LED_PIN: u32 = 5;
const BTN_PIN: u32 = 13; // external pull-up on the board; reads low when pressed

static BUTTON_HOOK: AtomicPtr<()> = AtomicPtr::new(null_mut());

pub fn init() {
    RCC_AHB1ENR.modify(|v| v | RCC_AHB1ENR_GPIOAEN | RCC_AHB1ENR_GPIOCEN);
    RCC_APB2ENR.modify(|v| v | RCC_APB2ENR_SYSCFGEN);
    let _ = RCC_APB2ENR.read(); // read back once to make sure the peripheral clocks are active

    // PA5 push-pull output
    GPIOA.moder().modify(|v| (v & !(3 << (LED_PIN * 2))) | (1 << (LED_PIN * 2)));

    // PC13 input, EXTI13 falling-edge interrupt
    GPIOC.moder().modify(|v| v & !(3 << (BTN_PIN * 2)));
    SYSCFG_EXTICR4.modify(|v| (v & !(0xF << 4)) | (0x2 << 4)); // EXTI13 <- GPIOC
    EXTI_FTSR.modify(|v| v | (1 << BTN_PIN));
    EXTI_IMR.modify(|v| v | (1 << BTN_PIN));

    nvic_set_priority(EXTI15_10_IRQN, 0x80);
    nvic_enable(EXTI15_10_IRQN);
}

pub fn led_toggle() {
    GPIOA.odr().modify(|v| v ^ (1 << LED_PIN));
}

pub fn button_is_pressed() -> bool {
    GPIOC.idr().read() & (1 << BTN_PIN) == 0
}

/// Register the button callback; it is called from the EXTI interrupt
pub fn set_button_handler(f: fn()) {
    BUTTON_HOOK.store(f as *mut (), Ordering::Release);
}

pub extern "C" fn exti15_10_handler() {
    if EXTI_PR.read() & (1 << BTN_PIN) != 0 {
        EXTI_PR.write(1 << BTN_PIN); // write 1 to clear the pending flag
        let hook = BUTTON_HOOK.load(Ordering::Acquire);
        if !hook.is_null() {
            // SAFETY: only set_button_handler writes it, so it always holds a fn()
            let f: fn() = unsafe { core::mem::transmute(hook) };
            f();
        }
    }
}
