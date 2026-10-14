
# Hexagon minivm

A Rust implementation of the
[Hexagon Virtual Machine specification](https://docs.qualcomm.com/bundle/publicresource/80-NB419-3_REV_A_Hexagin_Virtual_Machine_Specification.pdf)
([mirror](https://archive.is/yzlri)).
The Hexagon VM is a type-1 hypervisor and portability layer for Qualcomm
Hexagon DSPs.

## Status

minivm boots SMP Linux. On QEMU's `qcs6490-cdsp` machine it takes the place
of the firmware in the Hexagon Linux howto: the kernel brings up all six
CPUs, unpacks its initramfs, runs `/init` through its service scripts and
gives an interactive shell; `poweroff` shuts the machine down and QEMU exits
with status 0.

The guest tests, which exercise individual VM operations (the `trap1`
calls, TLB management, interrupt delivery, user-mode exceptions, several
virtual processors, timeouts), pass on the `virt` machine and on
hexagon-sim, the reference simulator, except that `test_mmu`'s result cannot
be read on hexagon-sim (see `sim-guest-tests` below).

The QEMU results are checked against the QEMU of the 23.1.2 Hexagon
toolchain release (tag `hexagon-sysemu-28-sept-2026` of
<https://github.com/qualcomm/qemu>), and hexagon-sim is the one in Hexagon
SDK 6.4.0.2 (tools 19.0.04).

What it does not do:

- **Linux on the `virt` machine is untested.** The kernels at hand for that
  machine reject its device tree with other firmware too, so there was
  nothing to compare against.
- **No scheduler.** Every virtual CPU is pinned to its own hardware thread,
  so a guest cannot have more virtual CPUs than the machine has threads, and
  there is one guest. This image uses at most the first eight threads.
- **V68-family cores only.** The build, context state, and maximum TLB page
  size are validated against the core revision at boot.
- **No single-stepping** of the guest (`GSSR.SS` is ignored), and the PMU
  calls are accepted and ignored.
- **Real hardware is untested.** It has run on QEMU and hexagon-sim only.
  The L2 QoS and ECC registers are left as they are.

## How it works

The machine starts minivm as firmware, with a device tree pointer in r1:r0
and the guest already loaded. minivm is linked at `0xff000000` and finds out
at run time where it was loaded, so the same image runs wherever the machine
has RAM for it (its ELF load address is `0x88f00000`, which suits the
QCS6490 CDSP machine).

Each virtual CPU is pinned to one hardware thread for life. A thread with
nothing to run waits in an idle loop. The guest's interrupt enable is
virtual: the core's own interrupts stay on whenever the guest runs, and
device interrupts are posted to a virtual interrupt controller. A thread
that must notice something is sent core interrupt 0, which is steered to it
with `iassignw`. One lock guards all monitor state.

At reset minivm invalidates the caches and turns the L1 caches on, unless
the loader left them on, in which case they may hold its writes and are
left alone. It then sizes the L2 from the config table and makes it
write-back. The guest's `vmcache` requests are carried out on
whole caches.

### What it takes from the device tree

Everything else about the platform comes from the core's config table, so
the device tree only has to say what that cannot:

| Where | What |
|-------|------|
| `/chosen/hvm,guest-entry` | Physical address the guest image is loaded and entered at. Without it, the start of the first `/memory` node; without a device tree, `0xa0000000`. |
| `/reserved-memory`, or a memory reservation | Must cover minivm's own image if any `/memory` node does, or minivm stops: the guest would take that memory for its own. |
| `/cpus/cpu@0`: `d-cache-size`, `d-cache-line-size`, `d-cache-sets` | L1 data cache geometry, which the by-index cache operations need. Without them, a default geometry (8 ways, 32-byte index steps), grown to the size the config table gives. |
| `next-level-cache` of `/cpus/cpu@0`: `cache-size` | L2 size, used only when the config table does not give one. |
| A node compatible with `arm,pl011` | The monitor's console. Without one, semihosting, until the guest starts. |

The device tree itself is passed to the guest in r0, unchanged.

| File | Purpose |
|------|---------|
| `src/entry.S` | Vectors, startup for the boot thread and the others, idle loop |
| `src/context.S` | Register save and restore around every exception |
| `src/main.rs` | Boot: discovery, device tree, starting the guest |
| `src/mon.rs` | Virtual CPUs, virtual interrupt controller, what runs next |
| `src/traps.rs` | The `trap1` interface, TLB fills, events forwarded to the guest |
| `src/hwint.rs` | L2VIC, the QTIMER behind `vmtimerop`, wake-up interrupts |
| `src/cache.rs` | Cache sizing at boot and the guest's `vmcache` requests |
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

## Running Linux

Get `vmlinux` and `initrd` as the Hexagon Linux howto describes, and use
minivm as the firmware:

```bash
qemu-system-hexagon -M qcs6490-cdsp -bios ./minivm \
    -kernel vmlinux -initrd initrd \
    -append 'console=ttyAMA0 mem=240M lpj=89124080 rdinit=/init' \
    -nographic -serial mon:stdio
```

minivm prints a few lines about what it found, then the kernel log follows.
Press Enter at `Please press Enter to activate this console.` for a shell.

On a QEMU whose `qcs6490-cdsp` machine does not take `-bios`, install minivm
under the name it looks for and point `-L` at it:

```bash
mkdir fw && cp minivm fw/hexagon_loadlinux_qcs6490_cdsp
qemu-system-hexagon -M qcs6490-cdsp -L $PWD/fw -dtb qcs6490-cdsp.dtb \
    -kernel vmlinux -initrd initrd -nographic -serial mon:stdio
```

## Testing

### Everything, from prebuilt downloads

```bash
make check
```

runs what CI runs (`.github/workflows/minivm-test.yml`) with nothing built
but minivm: `scripts/fetch-deps.sh` downloads the CodeLinaro Hexagon
toolchain (cross linker, clang, and the `qemu-system-hexagon` the howto
uses) and the howto's kernel and initramfs into `.deps`, checking each
against a pinned SHA-256. Then come the host unit tests, the guest tests
and two Linux boots (`LINUX_BOOT_REPEAT`). It needs `curl`, `zstd`,
`python3` and rustup; the QEMU binary is built for Ubuntu 22.04 and wants
the shared libraries the workflow installs. `make deps` only downloads.
`make check` removes prior guest-test result stamps before running, so changing
QEMU or its flags cannot silently reuse an older pass.

### Linux boot test

Boots Linux as above, checks the CPU count from the shell, runs a loop of
one-second sleeps and a burst of parallel fork/exec, and powers off:

```bash
make linux-boot QEMU=/path/to/qemu-system-hexagon KERNEL=vmlinux INITRD=initrd
```

`tests/linux_boot.py --repeat N` does it N times over, which is the way to
look for races.

### Guest tests

Small guests, run with minivm as the `virt` machine's kernel. They need
`clang` with the Hexagon target, `llvm-objcopy` and `qemu-system-hexagon`.
The `tests/*.S` ones are assembly. The `tests/hvm/*.c` ones are C, on a
small runtime (`tests/hvm/crt.S`, `lib.c`) that turns events into calls to
C, and check the interface operation by operation:

| Test | Covers |
|------|--------|
| `hvm_funcs` | the simple `trap1` calls, events from kernel and user mode, `trap1` from user mode and unassigned `trap1` numbers |
| `hvm_intop` | every `vmintop` operation, delivery as an event |
| `hvm_smp` | `vmstart`/`vmstop` on every thread, interrupts between CPUs, shared interrupt routing, the timer |
| `hvm_mmu` | every page size, `vmclrmap`, permission and user faults |
| `hvm_varadix` | the same through variable-radix tables (`vmnewmap` type 3), every leaf size from 4K to 16M, and the maps `vmnewmap` refuses |
| `hvm_timer` | a timeout on every CPU at once, 256 times each |

On QEMU, `hvm_timer` sees wake-ups milliseconds late, from QEMU running
the guest's CPUs on host time, so it only fails one later than a second;
on hexagon-sim the latest is about 13us.

```bash
make guest-tests
```

or the assembly ones through cargo (missing tools are reported as errors):

```bash
cargo test -p minivm-guest-tests --target x86_64-unknown-linux-gnu
```

The same tests run on hexagon-sim from a Hexagon SDK (set `HEXAGON_TOOLS`
to its `Tools` directory):

```bash
make sim-guest-tests
```

hexagon-sim loads one ELF, so `scripts/add_segment.py` adds each guest to a
copy of minivm as an extra segment. The simulator does not clear memory the
ELF does not cover, which the C runtime clears itself. `test_mmu` is not among them: it prints
PASS or FAIL from user mode, out of a page the simulator's semihosting cannot
translate, so nothing shows which.

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
cargo test --workspace --exclude minivm --exclude minivm-guest-tests \
    --target x86_64-unknown-linux-gnu
```

### Debugging

Start QEMU with `-s -S` and attach LLDB:

```bash
lldb target/hexagon-unknown-none-elf/release/minivm -o 'gdb-remote localhost:1234'
```

Once the guest is running, minivm reports a fatal error of its own on the
machine's PL011 before stopping every hardware thread; QEMU then exits with
status 1.

## License

This project is [licensed](LICENSE) under the BSD 3-clause "Clear" license.
