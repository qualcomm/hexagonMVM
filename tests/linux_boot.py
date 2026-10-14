#!/usr/bin/env python3
# Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
# SPDX-License-Identifier: BSD-3-Clause-Clear
"""Boot Linux under minivm on QEMU's qcs6490-cdsp machine and check it.

minivm takes the place of the firmware in the Hexagon Linux howto:

    qemu-system-hexagon -M qcs6490-cdsp -bios minivm \\
        -kernel vmlinux -initrd initrd \\
        -append 'console=ttyAMA0 mem=240M lpj=89124080 rdinit=/init' \\
        -nographic -serial mon:stdio

The test waits for the initramfs console, runs a few commands in its shell
and powers the machine off. It passes when every check does and QEMU exits
with status 0.

--repeat N runs the whole thing N times, which is how to look for races:
the checks are the same, only the timing differs from run to run.
"""

import argparse
import os
import pty
import re
import select
import subprocess
import sys
import time

APPEND = "console=ttyAMA0 mem=240M lpj=89124080 rdinit=/init"


class Guest:
    def __init__(self, cmd, log):
        master, slave = pty.openpty()
        self.proc = subprocess.Popen(cmd, stdin=slave, stdout=slave, stderr=slave, close_fds=True)
        os.close(slave)
        self.fd = master
        self.buf = b""
        self.log = log

    def wait_for(self, pattern, timeout):
        """Consume output up to and including `pattern`; None on timeout."""
        rx = re.compile(pattern.encode())
        end = time.time() + timeout
        while True:
            m = rx.search(self.buf)
            if m:
                out, self.buf = self.buf[: m.end()], self.buf[m.end() :]
                return out.decode("latin1").replace("\r", "")
            left = end - time.time()
            if left <= 0:
                return None
            ready, _, _ = select.select([self.fd], [], [], min(left, 1))
            if ready:
                try:
                    data = os.read(self.fd, 65536)
                except OSError:
                    return None
                self.buf += data
                if self.log:
                    self.log.write(data)
                    self.log.flush()
            elif self.proc.poll() is not None:
                return None

    def send(self, line):
        os.write(self.fd, line.encode() + b"\r")

    def run(self, command, timeout=60):
        """Run a shell command; return its output lines, or None if it hung.

        The output is bracketed by markers, because what comes back also
        holds the shell's echo of the command (wrapped at 80 columns) and
        whatever it had left to say about the previous one.
        """
        self.send(f"echo @@; {command}; echo rc=$?.")
        out = self.wait_for(r"\n@@\r*\n(?s:.*?)rc=\d+\.\s", timeout)
        if out is None:
            return None
        body = out.rsplit("\n@@\n", 1)[1]
        return [line for line in body.split("\n") if line and not re.fullmatch(r"rc=\d+\.", line)]

    def tail(self):
        return self.buf[-400:].decode("latin1").replace("\r", "")

    def kill(self):
        if self.proc.poll() is None:
            self.proc.kill()
        self.proc.wait()
        os.close(self.fd)


def boot_once(args, log):
    cmd = [
        args.qemu, "-M", "qcs6490-cdsp", "-bios", args.minivm,
        "-kernel", args.kernel, "-initrd", args.initrd,
        "-append", args.append, "-nographic", "-serial", "mon:stdio",
    ]
    if args.smp:
        cmd += ["-smp", str(args.smp)]
    cmd += args.qemu_arg
    cpus = args.smp or 6

    failures = []

    def check(name, ok, detail=""):
        if not ok:
            failures.append(name)
        if args.verbose or not ok:
            print(f"  {'ok  ' if ok else 'FAIL'} {name}{': ' + detail if detail and not ok else ''}")

    g = Guest(cmd, log)
    try:
        start = time.time()
        banner = g.wait_for(r"Please press Enter to activate this console\.", args.boot_timeout)
        check("boots to the initramfs console", banner is not None, g.tail())
        if banner is None:
            return failures
        boot_time = time.time() - start
        check(f"all {cpus} CPUs come up", f"Brought up 1 node, {cpus} CPU" in banner)

        g.send("")
        check("shell prompt", g.wait_for(r"# ", 30) is not None, g.tail())

        out = g.run("nproc")
        check(f"nproc is {cpus}", out == [str(cpus)], repr(out))

        out = g.run("uname -m")
        check("uname -m is hexagon", out == ["hexagon"], repr(out))

        # Timers and the console together: each line needs a console
        # interrupt, each sleep a timer interrupt on an idle system.
        n = args.ticks
        out = g.run(f"i=0; while [ $i -lt {n} ]; do i=$((i+1)); echo tick-$i; sleep 1; done",
                    timeout=n * 4 + 30)
        check(f"{n} one-second sleeps", out == [f"tick-{i + 1}" for i in range(n)], repr(out) + g.tail())

        # Every CPU doing fork/exec at once.
        out = g.run(
            f"for c in $(seq {cpus * 2}); do (for i in $(seq 20); do /bin/true; done; echo job-$c) & done; wait",
            timeout=180)
        check("parallel fork/exec",
              out is not None and sorted(out) == sorted(f"job-{c + 1}" for c in range(cpus * 2)),
              repr(out) + g.tail())

        g.send("poweroff")
        check("poweroff is requested", g.wait_for(r"reboot: Power down|Requesting system poweroff", 120) is not None,
              g.tail())
        try:
            rc = g.proc.wait(timeout=args.poweroff_timeout)
        except subprocess.TimeoutExpired:
            rc = None
        check("QEMU exits with status 0", rc == 0, f"exit status {rc}")
        if not failures:
            print(f"  PASS (console after {boot_time:.1f}s, {time.time() - start:.1f}s in all)")
    finally:
        g.kill()
    return failures


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--qemu", default="qemu-system-hexagon")
    ap.add_argument("--minivm", default="target/hexagon-unknown-none-elf/release/minivm")
    ap.add_argument("--kernel", required=True, help="the howto's vmlinux")
    ap.add_argument("--initrd", required=True, help="the howto's initrd")
    ap.add_argument("--append", default=APPEND, help="kernel command line")
    ap.add_argument("--smp", type=int, help="number of hardware threads (default: the machine's 6)")
    ap.add_argument("--repeat", type=int, default=1, help="boot this many times")
    ap.add_argument("--ticks", type=int, default=5, help="one-second sleeps to run")
    ap.add_argument("--boot-timeout", type=int, default=180)
    ap.add_argument("--poweroff-timeout", type=int, default=300)
    ap.add_argument("--log", help="write the console output here")
    ap.add_argument("--qemu-arg", action="append", default=[], help="extra QEMU argument (repeatable)")
    ap.add_argument("-v", "--verbose", action="store_true", help="report every check")
    args = ap.parse_args()

    log = open(args.log, "wb") if args.log else None
    failed = 0
    for i in range(args.repeat):
        print(f"boot {i + 1}/{args.repeat}")
        if log:
            log.write(f"\n===== boot {i + 1} =====\n".encode())
        if boot_once(args, log):
            failed += 1
    if args.repeat > 1:
        print(f"{args.repeat - failed}/{args.repeat} boots passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
