/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Linker script for minivm.
 *
 * The monitor runs at 0xff000000 (above Linux's FIXADDR_TOP) and is
 * loaded wherever the machine has RAM for it. The
 * startup code is position independent: it finds its own physical address
 * and wires a 1M page from the link address onto it, so the load address
 * below only has to be 1M aligned and backed by RAM.
 *
 * The load address suits the QCS6490 CDSP and QEMU virt machines. A different
 * platform can relink with another LOAD origin or rewrite every allocated
 * section's LMA consistently.
 */

MINIVM_LINK_ADDR = 0xff000000;
MINIVM_LOAD_ADDR = 0x88f00000;
MINIVM_IMAGE_SIZE = 0x100000;

/* The machine enters firmware at its physical address, with the MMU off. */
ENTRY(_start_phys)

MEMORY
{
    KERNEL (rwx) : ORIGIN = 0xff000000, LENGTH = 0x100000
    LOAD   (rwx) : ORIGIN = 0x88f00000, LENGTH = 0x100000
}

SECTIONS
{
    .start : ALIGN(4096)
    {
        KEEP(*(.start))
    } > KERNEL AT> LOAD

    _start_phys = LOADADDR(.start);

    .text : ALIGN(4096)
    {
        *(.text .text.*)
    } > KERNEL AT> LOAD

    .rodata : ALIGN(32)
    {
        *(.rodata .rodata.*)
    } > KERNEL AT> LOAD

    .data : ALIGN(32)
    {
        *(.sdata .sdata.*)
        *(.data .data.*)
        *(.got .got.*)
    } > KERNEL AT> LOAD

    .bss (NOLOAD) : ALIGN(32)
    {
        __bss_start = .;
        *(.sbss .sbss.*)
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
        __bss_end = .;
    } > KERNEL AT> LOAD

    __image_end = .;

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.ARM.exidx*)
    }
}

ASSERT(__image_end - MINIVM_LINK_ADDR <= MINIVM_IMAGE_SIZE,
       "minivm no longer fits the 1M page the startup code wires for it")
