/*
 * Linker script for guest test binaries.
 *
 * Places code at 0xa0000000 to match the minivm boot_guest() entry address
 * (GUEST_ENTRY_VA). The boot path uses identity offset mapping, so the
 * guest virtual address matches the physical address where the binary
 * is loaded via QEMU's -device loader.
 */
ENTRY(_start)

/*
 * The MMU tests build 4M-page PTEs per section, so each user section must
 * live in its own 4M region. .user_data2 must sit exactly 1M above
 * .user_data: they share one L1 entry that points to a 4-entry L2 table
 * of 1M pages (l2_pt[0] = user_data, l2_pt[1] = user_data2).
 */
SECTIONS {
    . = 0xa0000000;
    .start          : { *(.start) }
    .text           : { *(.text) }
    .rodata         : { *(.rodata*) }
    .data           : { *(.data) }
    .bss            : { __bss_start = .; *(.bss .bss.* COMMON) __bss_end = .; }
    . = ALIGN(0x400000);
    .user_text      : { *(.user_text) }
    . = ALIGN(0x400000);
    .user_rodata    : { *(.user_rodata) }
    . = ALIGN(0x400000);
    .user_data      : { *(.user_data) }
    . = ALIGN(0x100000);
    .user_data2     : { *(.user_data2) }
    /DISCARD/       : { *(.note.ABI-tag) *(.comment) *(.note*) }
}
