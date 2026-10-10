/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Linker script for minivm.
 * Kernel is linked at 0xff000000.
 */

ENTRY(_start)

MEMORY
{
    KERNEL (rwx) : ORIGIN = 0xff000000, LENGTH = 8M
}

SECTIONS
{
    .start : ALIGN(4096)
    {
        KEEP(*(.start))
    } > KERNEL

    .text : ALIGN(4096)
    {
        *(.text .text.*)
    } > KERNEL

    .rodata : ALIGN(32)
    {
        *(.rodata .rodata.*)
    } > KERNEL

    .data : ALIGN(32)
    {
        __global_pointer = .;
        *(.sdata .sdata.*)
        *(.data .data.*)
    } > KERNEL

    .bss (NOLOAD) : ALIGN(32)
    {
        __bss_start = .;
        *(.sbss .sbss.*)
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4);
        __bss_end = .;
    } > KERNEL

    .stack (NOLOAD) : ALIGN(32)
    {
        __stack_start = .;
        . += 0x100000;  /* 1MB kernel stack */
        __stack_end = .;
    } > KERNEL

    .heap (NOLOAD) : ALIGN(32)
    {
        __heap_start = .;
        . += 0x20000;  /* 128KB kernel heap */
        __heap_end = .;
    } > KERNEL

    /DISCARD/ :
    {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
