/* STM32F446RE: 512 KB Flash, 128 KB SRAM */

ENTRY(Reset_Handler)

_Min_Stack_Size = 0x800;   /* MSP: used by main() before os::start, then by all interrupts */

MEMORY
{
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 512K
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 128K
}

_estack = ORIGIN(RAM) + LENGTH(RAM);

SECTIONS
{
    /* Vector table entry 0 is the initial MSP. A linker symbol is awkward to put into a
       Rust array of function pointers, so it is written here instead. */
    .isr_vector :
    {
        LONG(_estack);
        KEEP(*(.isr_vector))
        . = ALIGN(4);
    } > FLASH

    .text :
    {
        . = ALIGN(4);
        *(.text .text.*)
        . = ALIGN(4);
    } > FLASH

    .rodata :
    {
        . = ALIGN(4);
        *(.rodata .rodata.*)
        . = ALIGN(4);
    } > FLASH

    .ARM.exidx : { *(.ARM.exidx .ARM.exidx.*) } > FLASH

    _sidata = LOADADDR(.data);

    .data :
    {
        . = ALIGN(4);
        _sdata = .;
        *(.data .data.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT> FLASH

    .bss (NOLOAD) :
    {
        . = ALIGN(4);
        _sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4);
        _ebss = .;
    } > RAM

    /* Placeholder only: if .data + .bss + MSP don't fit, linking fails */
    ._user_stack (NOLOAD) :
    {
        . = ALIGN(8);
        . = . + _Min_Stack_Size;
    } > RAM

    .ARM.attributes 0 : { *(.ARM.attributes) }
}
