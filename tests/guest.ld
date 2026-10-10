/*
 * Linker script for guest test binaries.
 *
 * Places code at 0xa0000000 to match the minivm boot_guest() entry address
 * (GUEST_ENTRY_VA). The boot path uses identity offset mapping, so the
 * guest virtual address matches the physical address where the binary
 * is loaded via QEMU's -device loader.
 */
ENTRY(_start)

SECTIONS {
    . = 0xa0000000;
    .start          : { *(.start) }
    .text           : { *(.text) }
    .rodata         : { *(.rodata*) }
    .data           : { *(.data) }
    .bss            : { *(.bss) }
    .user_text      : { *(.user_text) }
    .user_rodata    : { *(.user_rodata) }
    .user_data      : { *(.user_data) }
    .user_data2     : { *(.user_data2) }
    /DISCARD/       : { *(.note.ABI-tag) *(.comment) *(.note*) }
}
