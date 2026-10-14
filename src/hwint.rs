/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Physical interrupts: the L2VIC, the QTIMER behind `vmtimerop`, and the
//! inter-thread wake-up interrupts.
//!
//! Interrupt numbering: 0..31 are core interrupts and 32 + n is
//! L2VIC line n. A device line is passed through to the guest under the same
//! number, as a shared virtual interrupt. The QTIMER line belongs to the
//! monitor.

use crate::arch::{self, mmio_read, mmio_write};
use crate::mon::{Mon, IPI_INT, L2_CORE_INT, MAX_HT, PERCPU_INTS, TIME_GUESTINT};

// L2VIC registers. Each bitmap has one word per 32 lines.
const L2VIC_VID: u32 = 0x000;
const L2VIC_ENABLE_CLEAR: u32 = 0x180;
const L2VIC_ENABLE_SET: u32 = 0x200;
const L2VIC_TYPE: u32 = 0x280;
const L2VIC_STATUS: u32 = 0x380;
const L2VIC_CLEAR: u32 = 0x400;
/// Lines the monitor manages.
const L2VIC_LINES: u32 = 256;

/// L2VIC line of the QTIMER frame the monitor uses.
const TIMER_LINE: u32 = 2;

// QTIMER: a control block, then one frame per timer.
const QT_CNTFRQ: u32 = 0x000;
const QT_CNTSR: u32 = 0x004;
const QT_CNTACR: u32 = 0x040;
const QT_FRAME: u32 = 0x1000;
const QTF_CNT_LO: u32 = 0x00;
const QTF_CNT_HI: u32 = 0x04;
const QTF_CVAL_LO: u32 = 0x20;
const QTF_CVAL_HI: u32 = 0x24;
const QTF_CTL: u32 = 0x2c;

pub const TIMER_HZ: u64 = 19_200_000;
pub const NS_PER_TICK: u64 = 52;
/// Shortest timeout the timer is asked for, in ticks.
const TICK_GRANULARITY: u64 = 4;

/// Virtual addresses of the L2VIC and of the QTIMER control block, set once
/// by `init` before any other thread runs.
static mut L2VIC: u32 = 0;
static mut QTIMER: u32 = 0;
/// Lines currently enabled at the L2VIC. Guarded by the monitor lock.
static mut ENABLED: [u32; (L2VIC_LINES / 32) as usize] = [0; (L2VIC_LINES / 32) as usize];

fn l2vic(reg: u32, line: u32) -> u32 {
    unsafe { L2VIC + reg + (line / 32) * 4 }
}

fn qtimer_frame(reg: u32) -> u32 {
    unsafe { QTIMER + QT_FRAME + reg }
}

/// Bring up the interrupt controller and the timer.
///
/// # Safety
/// The device window must be mapped at these addresses, and no other thread
/// may be running yet.
pub unsafe fn init(l2vic_va: u32, qtimer_va: u32) {
    L2VIC = l2vic_va;
    QTIMER = qtimer_va;

    // Every line off, nothing active, level triggered. The guest turns its
    // lines on as it claims them.
    for line in (0..L2VIC_LINES).step_by(32) {
        mmio_write(l2vic(L2VIC_ENABLE_CLEAR, line), !0);
        mmio_write(l2vic(L2VIC_CLEAR, line), !0);
        mmio_write(l2vic(L2VIC_TYPE, line), 0);
    }

    mmio_write(QTIMER + QT_CNTSR, !0);
    mmio_write(QTIMER + QT_CNTACR, !0);
    mmio_write(QTIMER + QT_CNTFRQ, TIMER_HZ as u32);
    set_match(!0);
    // The frame has to be enabled for a match to raise its line at all.
    mmio_write(qtimer_frame(QTF_CTL), 1);

    ENABLED[(TIMER_LINE / 32) as usize] |= 1 << TIMER_LINE;
    mmio_write(l2vic(L2VIC_ENABLE_SET, TIMER_LINE), 1 << TIMER_LINE);
}

/// Per-thread interrupt setup: the boot thread takes the L2VIC's interrupt,
/// and no thread takes the wake-up interrupt until it is sent one.
///
/// # Safety
/// Must run on the thread being set up, in monitor mode.
pub unsafe fn init_thread(cpu: usize) {
    // The wake-up interrupt starts masked too: `Mon::kick` steers it.
    let accept = if cpu == 0 { 1u32 << L2_CORE_INT } else { 0 };
    arch::set_imask(!accept & 0xFFFF);
}

fn set_match(ticks: u64) {
    unsafe {
        // High half first, so the compare value never transiently matches.
        mmio_write(qtimer_frame(QTF_CVAL_HI), !0);
        mmio_write(qtimer_frame(QTF_CVAL_LO), ticks as u32);
        mmio_write(qtimer_frame(QTF_CVAL_HI), (ticks >> 32) as u32);
    }
}

pub fn read_count() -> u64 {
    unsafe {
        loop {
            let hi = mmio_read(qtimer_frame(QTF_CNT_HI));
            let lo = mmio_read(qtimer_frame(QTF_CNT_LO));
            if hi == mmio_read(qtimer_frame(QTF_CNT_HI)) {
                return ((hi as u64) << 32) | lo as u64;
            }
        }
    }
}

impl Mon {
    /// Current time in timer ticks.
    pub fn now(&mut self) -> u64 {
        let now = read_count();
        if now > self.last_ticks {
            self.last_ticks = now;
        }
        now
    }

    /// Program the timer for the earliest virtual CPU timeout.
    fn rearm_timer(&mut self) {
        let next = self
            .cpus
            .iter()
            .filter(|c| c.timeout != 0)
            .map(|c| c.timeout)
            .min()
            .unwrap_or(!0);
        self.next_ticks = next;
        set_match(next);
    }

    /// Set `cpu`'s timeout to an absolute tick (0 cancels) and return what
    /// was set. A time that has already passed cancels too.
    fn set_timeout_tick(&mut self, cpu: usize, mut tick: u64) -> u64 {
        if tick <= self.last_ticks {
            tick = 0;
        }
        self.cpus[cpu].timeout = tick;
        self.rearm_timer();
        tick
    }

    /// The timer fired: post the timer interrupt to every virtual CPU whose
    /// timeout has come, and aim the timer at the next one. Rewriting the
    /// compare value is also what makes the QTIMER drop its (level) line.
    fn timer_interrupt(&mut self, me: usize) {
        let now = self.now() + TICK_GRANULARITY;
        for cpu in 0..MAX_HT {
            let t = self.cpus[cpu].timeout;
            if t != 0 && t <= now {
                self.cpus[cpu].timeout = 0;
                self.post(me, cpu, TIME_GUESTINT);
            }
        }
        self.rearm_timer();
    }

    /// `vmtimerop`. Times cross the interface in nanoseconds.
    pub fn timerop(&mut self, cpu: usize, op: u32, arg: u64) -> u64 {
        const FOREVER: u64 = !0;
        match op {
            0 => TIMER_HZ * NS_PER_TICK,
            1 => NS_PER_TICK,
            2 => self.now() * NS_PER_TICK,
            3 => self.cpus[cpu].timeout * NS_PER_TICK,
            4 => {
                let tick = if arg == FOREVER { 0 } else { arg / NS_PER_TICK };
                self.now();
                self.set_timeout_tick(cpu, tick) * NS_PER_TICK
            }
            5 => {
                let tick = if arg == FOREVER {
                    0
                } else {
                    self.now() + (arg / NS_PER_TICK).max(TICK_GRANULARITY)
                };
                self.set_timeout_tick(cpu, tick) * NS_PER_TICK
            }
            _ => !0,
        }
    }

    /// Drop a stopped virtual CPU's timeout.
    pub fn cancel_timeout(&mut self, cpu: usize) {
        self.cpus[cpu].timeout = 0;
        self.rearm_timer();
    }

    /// Deal with a line the L2VIC is presenting.
    fn service_line(&mut self, me: usize, line: u32) {
        if line == TIMER_LINE {
            self.timer_interrupt(me);
            // The L2VIC turns a line off when it delivers it (QEMU's model
            // does not); the timer's compare value has moved on, so turn it
            // back on.
            unsafe { mmio_write(l2vic(L2VIC_ENABLE_SET, TIMER_LINE), 1 << TIMER_LINE) };
            return;
        }
        // A device line stays off from here until the guest has handled the
        // interrupt and re-enables it (`guest_enabled`), so a level that is
        // still asserted does not interrupt again in between.
        unsafe {
            ENABLED[(line / 32) as usize] &= !(1 << (line % 32));
            mmio_write(l2vic(L2VIC_ENABLE_CLEAR, line), 1 << (line % 32));
        }
        self.shint_post(me, 32 + line - PERCPU_INTS);
    }

    /// Acknowledge core interrupts.
    ///
    /// `ciad` re-arms the core interrupts in `mask`, and `ciad` of the
    /// L2VIC's interrupt is also the controller's acknowledge: it forgets
    /// the line it is presenting and moves on to the next.
    ///
    /// On hexagon-sim that is all. QEMU up to at least the 23.1.2
    /// toolchain's acknowledges on every `ciad`, whatever its mask
    /// (`standalone_systests/src/ciad_l2vic.c` in qemu-hexagon-testing
    /// shows the difference; fixed by QEMU
    /// commit "target/hexagon: acknowledge the l2vic only on ciad of a
    /// taken l2vic interrupt"). There, a thread acknowledging its wake-up
    /// interrupt can retire a device line nobody has looked at, and that
    /// interrupt is gone for good.
    ///
    /// Hence the rule: whoever is about to `ciad` first services the line
    /// the controller is presenting. While one is presented the controller
    /// presents nothing else, so that much is race free. When none is, a
    /// line could be presented between looking and the `ciad`; to close
    /// that window every line is disabled around the `ciad`, which makes
    /// the controller hold new arrivals as pending until they are enabled
    /// again.
    ///
    /// The boot thread, which takes the L2VIC's own interrupt, can find the
    /// line already serviced by another thread. That is fine. Where only
    /// the L2VIC's `ciad` acknowledges, all of this is merely unnecessary.
    pub fn ack(&mut self, me: usize, mask: u32) {
        if let Some(line) = active_line() {
            self.service_line(me, line);
            unsafe { arch::ciad(mask) };
            return;
        }
        let enabled = unsafe { ENABLED };
        unsafe {
            for (word, &lines) in enabled.iter().enumerate() {
                if lines != 0 {
                    mmio_write(L2VIC + L2VIC_ENABLE_CLEAR + word as u32 * 4, lines);
                }
            }
        }
        if let Some(line) = active_line() {
            self.service_line(me, line);
        }
        // Servicing a device line turns it off, so read the set again.
        let enabled = unsafe { ENABLED };
        unsafe {
            arch::ciad(mask);
            for (word, &lines) in enabled.iter().enumerate() {
                if lines != 0 {
                    mmio_write(L2VIC + L2VIC_ENABLE_SET + word as u32 * 4, lines);
                }
            }
        }
    }
}

/// The line the L2VIC is presenting to the core, if any.
fn active_line() -> Option<u32> {
    unsafe {
        let vid = mmio_read(L2VIC + L2VIC_VID) & 0x3ff;
        if vid < L2VIC_LINES && mmio_read(l2vic(L2VIC_STATUS, vid)) & (1 << (vid % 32)) != 0 {
            return Some(vid);
        }
        for line in (0..L2VIC_LINES).step_by(32) {
            let status = mmio_read(l2vic(L2VIC_STATUS, line));
            if status != 0 {
                return Some(line + status.trailing_zeros());
            }
        }
    }
    None
}

/// The guest enabled virtual interrupt `intno`: turn its device line back
/// on. The caller holds the monitor lock.
pub fn guest_enabled(intno: u32) {
    if intno < 32 {
        return;
    }
    let line = intno - 32;
    if line < L2VIC_LINES && line != TIMER_LINE {
        unsafe {
            ENABLED[(line / 32) as usize] |= 1 << (line % 32);
            mmio_write(l2vic(L2VIC_ENABLE_SET, line), 1 << (line % 32));
        }
    }
}

/// Core interrupt entry.
pub fn interrupt(m: &mut Mon, cpu: usize, core_int: u32) {
    // A wake-up interrupt needs no more than acknowledging: the reason for
    // it is in the state `Mon::leave` looks at.
    if core_int == IPI_INT {
        m.ipi_received(cpu);
    } else {
        m.ack(cpu, 1 << core_int);
    }
}
