# Booting Zephyr with minivm on QEMU

This document explains how to build minivm and use it to boot a Zephyr
application on `qemu-system-hexagon`.

## Prerequisites

- **Rust nightly** with `-Zbuild-std` support
- **LLVM/Clang** with Hexagon target (clang, lld, llvm-objcopy)
- **qemu-system-hexagon** built with the `virt` machine
- **Zephyr** source tree and `west` tool (for building Zephyr applications)

## Building minivm

minivm is a `#![no_std]` Rust binary targeting `hexagon-unknown-none-elf`.
The `.cargo/config.toml` sets this as the default target, so the build
commands use `-Zbuild-std` to compile `core` and `alloc` from source.

### Debug build

```bash
cargo +nightly build \
    -Zbuild-std=core,alloc \
    -Zbuild-std-features=compiler-builtins-mem
```

Or equivalently:

```bash
make minivm
```

The output binary is `target/hexagon-unknown-none-elf/debug/minivm`.

### Release build

```bash
cargo +nightly build \
    -Zbuild-std=core,alloc \
    -Zbuild-std-features=compiler-builtins-mem \
    --release
```

The output binary is `target/hexagon-unknown-none-elf/release/minivm`.

## Building a Zephyr application

Build Zephyr's hello\_world sample for the `qemu_hexagon_virt` board:

```bash
cd /path/to/zephyr
ZEPHYR_TOOLCHAIN_VARIANT=host \
LLVM_TOOLCHAIN_PATH=/path/to/llvm \
west build -b qemu_hexagon/qemu_hexagon_virt samples/hello_world \
    --pristine \
    -- -DTOOLCHAIN_VARIANT_COMPILER=llvm
```

The output binary is `build/zephyr/zephyr.bin`.

### Toolchain notes

The Hexagon Zephyr board uses the `host` toolchain variant with
`TOOLCHAIN_VARIANT_COMPILER=llvm` to select clang instead of gcc.
Set `LLVM_TOOLCHAIN_PATH` to the root of your LLVM installation
(the directory containing `bin/clang`).

## Booting Zephyr with minivm

minivm acts as a guest bootloader: it configures a child VM with identity
offset translation and boots the guest binary at virtual address
`0xa0000000`.  QEMU's `-device loader` places the guest binary at that
physical address before minivm starts.

### Quick start

```bash
qemu-system-hexagon -M virt -nographic -m 4G \
    -kernel target/hexagon-unknown-none-elf/debug/minivm \
    -device "loader,addr=0xa0000000,file=/path/to/zephyr/build/zephyr/zephyr.bin"
```

Or using the Makefile convenience target:

```bash
make zephyr-boot ZEPHYR_BIN=/path/to/zephyr/build/zephyr/zephyr.bin
```

### Expected output

```
minivm: Hexagon VM (Rust)
  [init] kernel globals
  [init] scheduler
  [init] ASID table
  [init] interrupt controller
  [init] timer
  [init] futex table
  [init] thread contexts
  [init] boot VM
  [init] memory layout
guest: start boot
guest: vm configured
...
guest: booting
*** Booting Zephyr OS build v4.4.0-rc1-... ***
Hello World! qemu_hexagon/qemu_hexagon_virt
```

The QEMU process will stay running after Zephyr prints its output
(Zephyr enters the idle loop).  Press `Ctrl-A X` to exit QEMU, or
run with `timeout 30 ...` to auto-terminate.

## How it works

1. QEMU loads minivm at its link address (`0x20000000`, the start of
   Hexagon SRAM) and the guest binary at `0xa0000000` via `-device loader`.

2. minivm initializes its subsystems (scheduler, interrupt controller,
   timer, ASID table, TLB) and creates a child VM via the Hexagon VM
   `CONFIG` and `VMOP_BOOT` trap sequence.

3. The child VM is configured with:
   - **Offset translation** with `pages=0` (identity mapping: guest
     virtual addresses equal physical addresses).
   - **Fences** from `0x00000000` to `0xFE000000` (full address space
     minus device region).
   - **288 physical interrupts** identity-mapped to virtual interrupts.
   - Guest entry point at `0xa0000000` (where QEMU placed the binary).

4. On `VMOP_BOOT`, minivm context-switches to the guest via `crswap`
   and the guest begins executing its reset handler.

5. Guest traps (trap0, trap1, TLB misses, interrupts) return to minivm
   for servicing.  minivm handles VM operations, timer management,
   interrupt delivery, and page table walks, then returns to the guest
   via `vmrte`.

## Using minivm as a Zephyr `west build -t run` backend

Zephyr's `qemu_hexagon_virt` board runner looks for the `loadlinux`
binary via the `HEXAGON_H2_LOADLINUX` environment variable.  Point it
at minivm to use `west build -t run` directly:

```bash
export HEXAGON_H2_LOADLINUX=/path/to/minivm
west build -t run
```

## Debugging

Start QEMU with GDB server flags and attach LLDB:

```bash
# Terminal 1: start QEMU paused
qemu-system-hexagon -M virt -nographic -m 4G -s -S \
    -kernel target/hexagon-unknown-none-elf/debug/minivm \
    -device "loader,addr=0xa0000000,file=zephyr.bin"

# Terminal 2: attach debugger
lldb target/hexagon-unknown-none-elf/debug/minivm \
    -o 'gdb-remote localhost:1234' \
    -o 'break set -n minivm_main' \
    -o c
```

To debug the Zephyr guest, set breakpoints at guest addresses
(e.g., `break set -a 0xa0000000` for the guest entry point).
