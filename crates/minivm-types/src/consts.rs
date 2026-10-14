/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Constants shared by the active monitor and memory library.

// ASID
pub const ASID_BITS: u32 = 7; // v60+
pub const MAX_ASIDS: u32 = 1 << ASID_BITS;
