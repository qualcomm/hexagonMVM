
# Hexagon minivm

A Rust implementation of the
[Hexagon Virtual Machine specification](https://docs.qualcomm.com/bundle/publicresource/80-NB419-3_REV_A_Hexagin_Virtual_Machine_Specification.pdf)
([mirror](https://archive.is/yzlri)).
The Hexagon VM is a type-1 hypervisor and portability layer for Qualcomm
Hexagon DSPs.

## Status

minivm boots a full Linux kernel (arm64 Image loaded as a flat binary) through
all initcalls to the `/init` handoff. Guest tests exercise individual VM
operations (trap1 calls, TLB management, interrupt delivery, user-mode
exceptions).

## Project structure

The top-level crate (`src/main.rs`) is the `#![no_std]` hypervisor binary,
built for `hexagon-unknown-none-elf`. Subsystem logic lives in workspace
crates under `crates/`, each with host-runnable unit tests:

| Crate | Purpose |
|-------|---------|
| `minivm-types` | Shared constants, register layouts, error codes |
| `minivm-arch` | TLB, cache, and architectural helpers |
| `minivm-mem` | Physical/virtual memory management |
| `minivm-sync` | Spinlocks and synchronization primitives |
| `minivm-sched` | Scheduler data structures (ready/run lists) |
| `minivm-thread` | Thread context and state |
| `minivm-vm` | VM configuration and VMID management |
| `minivm-trap` | Trap decoding and dispatch tables |
| `minivm-event` | Event/interrupt routing |
| `minivm-intc` | Interrupt controller abstraction |
| `minivm-timer` | Timer management |
| `minivm-power` | Power state tracking |
| `minivm-init` | Initialization sequences |
| `minivm-guest-tests` | Integration tests that build and run guest binaries on QEMU |

## Building

### Prerequisites

- Rust nightly (for `-Zbuild-std`)
- `hexagon-unknown-none-elf` target support

### Debug build

```bash
cargo +nightly build -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
```

### Release build

```bash
cargo +nightly build -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --release
```

## Testing

### Host unit tests

The workspace crates have 248+ unit tests that run on the host:

```bash
cargo test -p minivm-types -p minivm-mem -p minivm-timer -p minivm-init \
    -p minivm-power -p minivm-sched -p minivm-sync -p minivm-trap \
    -p minivm-vm -p minivm-event -p minivm-arch \
    --all-targets --target x86_64-unknown-linux-gnu
```

### Guest integration tests

Guest tests build Hexagon assembly programs and run them on QEMU with minivm
as the kernel. They require `clang` (with Hexagon target), `llvm-objcopy`, and
`qemu-system-hexagon`.

Via cargo (auto-skips when tools are missing):

```bash
cargo test -p minivm-guest-tests --target x86_64-unknown-linux-gnu
```

Via make (for standalone use):

```bash
make guest-tests
```

### Running with Linux

```bash
qemu-system-hexagon -M virt -nographic \
    -kernel ./minivm \
    -device "loader,addr=0xa0000000,file=./vmlinux.bin"
```

### Debugging

Attach LLDB by starting QEMU with GDB server flags:

```bash
qemu-system-hexagon -M virt -nographic -s -S \
    -kernel ./minivm \
    -device "loader,addr=0xa0000000,file=./vmlinux.bin"
```

Then in another terminal:

```bash
lldb -- -o 'gdb-remote localhost:1234' -o 'break set -a 0x20000000' -o c
```

## License

This project is [licensed](LICENSE) under the BSD 3-clause "Clear" license.
