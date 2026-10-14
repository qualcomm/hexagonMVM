
# Hexagon minivm

A Rust implementation of the
[Hexagon Virtual Machine specification](https://docs.qualcomm.com/bundle/publicresource/80-NB419-3_REV_A_Hexagin_Virtual_Machine_Specification.pdf)
([mirror](https://archive.is/yzlri)).
The Hexagon VM is a type-1 hypervisor and portability layer for Qualcomm
Hexagon DSPs.

## Status

minivm runs a guest with a virtual CPU on every hardware thread. The guest
tests, which exercise individual VM operations (the `trap1` calls, TLB
management, interrupt delivery, user-mode exceptions, several virtual
processors), pass on QEMU's `virt` machine.

## How it works

The machine starts minivm as firmware, with the guest already loaded.
minivm is linked at `0xff000000` and finds out at run time where it was
loaded, so the same image runs wherever the machine has RAM for it (its ELF
load address is `0x88f00000`, which suits the QCS6490 CDSP machine).

Each virtual CPU is pinned to one hardware thread for life. A thread with
nothing to run waits in an idle loop. The guest's interrupt enable is
virtual: the core's own interrupts stay on whenever the guest runs, and
device interrupts are posted to a virtual interrupt controller. A thread
that must notice something is sent core interrupt 0, which is steered to it
with `iassignw`. One lock guards all monitor state.

| File | Purpose |
|------|---------|
| `src/entry.S` | Vectors, startup for the boot thread and the others, idle loop |
| `src/context.S` | Register save and restore around every exception |
| `src/main.rs` | Boot: discovery, starting the guest |
| `src/mon.rs` | Virtual CPUs, virtual interrupt controller, what runs next |
| `src/traps.rs` | The `trap1` interface, TLB fills, events forwarded to the guest |
| `src/hwint.rs` | L2VIC, the QTIMER behind `vmtimerop`, wake-up interrupts |
| `src/console.rs` | The monitor's own output |

The workspace crates under `crates/` hold logic with host-runnable unit
tests. The monitor uses `minivm-types` (register layouts, constants) and
`minivm-mem` (ASID table, guest page-table walks, TLB entry format), which
shares the one hardware-TLB representation in `minivm-types`.

## Building

### Prerequisites

- The Rust nightly pinned in `rust-toolchain.toml` (for `-Zbuild-std`)
- `hexagon-unknown-none-elf-ld.lld` on `PATH` (from a Hexagon LLVM
  toolchain), or `CC` set to a Hexagon clang for `make`

```bash
make minivm          # release build: target/hexagon-unknown-none-elf/release/minivm
make minivm-debug    # debug build
```

## Testing

### Guest tests

Small guests, run with minivm as the `virt` machine's kernel. They need
`clang` with the Hexagon target, `llvm-objcopy` and `qemu-system-hexagon`.
The `tests/*.S` ones are assembly.

```bash
make guest-tests
```

A flat guest image is loaded at `0xa0000000` and entered there under an
identity map:

```bash
qemu-system-hexagon -M virt -bios none -nographic \
    -kernel ./minivm \
    -device "loader,addr=0xa0000000,file=./guest.bin"
```

`-bios none` keeps QEMU from loading its bundled firmware. The machine
needs RAM at minivm's load address, which the default 4G provides.

### Host unit tests

```bash
cargo test --workspace --exclude minivm \
    --target x86_64-unknown-linux-gnu
```

### Debugging

Start QEMU with `-s -S` and attach LLDB:

```bash
lldb target/hexagon-unknown-none-elf/release/minivm -o 'gdb-remote localhost:1234'
```

## License

This project is [licensed](LICENSE) under the BSD 3-clause "Clear" license.
