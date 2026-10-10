/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_thread::create::{validate_create, CreateParams, ThreadInit};

pub fn run() {
    debug::write0(b"  [test] thread creation\n\0");

    let params = CreateParams {
        pc: 0x1000,
        sp: 0x8000,
        arg1: 42,
        prio: 10,
    };
    assert!(validate_create(&params, 0).is_ok());

    let init = ThreadInit::from_params(&params);
    assert_eq!(init.prio, 10);
    assert_eq!(init.elr, 0x1000);
    assert_eq!(init.sp, 0x8000);
    assert_eq!(init.arg, 42);

    // Bad SP alignment
    let bad_params = CreateParams {
        pc: 0x1000,
        sp: 0x8001,
        arg1: 0,
        prio: 10,
    };
    assert!(validate_create(&bad_params, 0).is_err());

    // Bad PC alignment
    let bad_params = CreateParams {
        pc: 0x1001,
        sp: 0x8000,
        arg1: 0,
        prio: 10,
    };
    assert!(validate_create(&bad_params, 0).is_err());

    debug::write0(b"  [test] thread creation OK\n\0");
}
