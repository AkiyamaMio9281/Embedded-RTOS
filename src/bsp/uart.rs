//! USART2 (PA2/PA3), wired to the on-board ST-LINK virtual COM port.
//! Polled transmit, independent of the RTOS; for printing from several tasks use `os::Mutex<Uart>`.

use core::fmt;

use super::board::PCLK1_HZ;
use super::regs::*;

pub fn init(baud: u32) {
    RCC_AHB1ENR.modify(|v| v | RCC_AHB1ENR_GPIOAEN);
    RCC_APB1ENR.modify(|v| v | RCC_APB1ENR_USART2EN);
    let _ = RCC_APB1ENR.read();

    // PA2 = TX, PA3 = RX, alternate function AF7
    GPIOA.moder().modify(|v| (v & !(0xF << 4)) | (0xA << 4));
    GPIOA.afrl().modify(|v| (v & !(0xFF << 8)) | (0x77 << 8));

    // With 16x oversampling, BRR = fPCLK / baud
    usart2::BRR.write((PCLK1_HZ + baud / 2) / baud);
    usart2::CR1.write(USART_CR1_UE | USART_CR1_TE | USART_CR1_RE);
}

/// UART handle. A zero-sized type implementing `fmt::Write`, so you can `writeln!(Uart, ...)`
pub struct Uart;

impl Uart {
    pub fn write_byte(b: u8) {
        while usart2::SR.read() & USART_SR_TXE == 0 {}
        usart2::DR.write(b as u32);
    }
}

impl fmt::Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                Uart::write_byte(b'\r');
            }
            Uart::write_byte(b);
        }
        Ok(())
    }
}
