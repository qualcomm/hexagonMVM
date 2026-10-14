#!/usr/bin/env python3
# Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
# SPDX-License-Identifier: BSD-3-Clause-Clear
"""Add a flat binary to a 32-bit little-endian ELF executable as one more
PT_LOAD segment, loaded at ADDR (physical and virtual).

hexagon-sim loads a single ELF, so this is how minivm and a guest image go
in together:

    add_segment.py minivm guest.bin 0xa0000000 minivm+guest.elf

The program header table is rewritten at the end of the file; nothing else
in the input moves.
"""

import struct
import sys


def main():
    if len(sys.argv) != 5:
        sys.exit(__doc__)
    elf_in, bin_in, addr, out = sys.argv[1], sys.argv[2], int(sys.argv[3], 0), sys.argv[4]
    elf = bytearray(open(elf_in, "rb").read())
    data = open(bin_in, "rb").read()
    if elf[:4] != b"\x7fELF" or elf[4] != 1 or elf[5] != 1:
        sys.exit(f"{elf_in}: not a 32-bit little-endian ELF")

    (phoff,) = struct.unpack_from("<I", elf, 28)
    phentsize, phnum = struct.unpack_from("<HH", elf, 42)
    phdrs = elf[phoff : phoff + phentsize * phnum]

    data_off = (len(elf) + 0xFFF) & ~0xFFF
    elf += b"\0" * (data_off - len(elf)) + data
    new_phoff = (len(elf) + 3) & ~3
    elf += b"\0" * (new_phoff - len(elf))
    PT_LOAD, PF_RWX = 1, 7
    elf += phdrs + struct.pack("<IIIIIIII", PT_LOAD, data_off, addr, addr, len(data), len(data), PF_RWX, 0x1000)
    struct.pack_into("<I", elf, 28, new_phoff)
    struct.pack_into("<H", elf, 44, phnum + 1)
    open(out, "wb").write(elf)


if __name__ == "__main__":
    main()
