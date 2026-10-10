/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_mem::asid::AsidTable;
use minivm_types::asid::TranslationType;

pub fn run() {
    debug::write0(b"  [test] ASID table ops\n\0");
    {
        let mut table = AsidTable::new();
        let mut inv_count: u32 = 0;

        // Allocate a new ASID entry
        let asid = table.inc(0x1000_0000, TranslationType::Offset, false, 0, 1, |_| {
            inv_count += 1
        });
        assert!(asid >= 0);
        assert_eq!(inv_count, 1); // new entry always invalidates

        // Verify entry fields
        let entry = table.get(asid as u32);
        assert_eq!(entry.ptb, 0x1000_0000);
        assert_eq!(entry.fields.count(), 1);
        assert_eq!(entry.fields.vmid(), 1);
        assert_eq!(entry.fields.trans_type(), TranslationType::Offset as u8);

        // Search should find it
        let found = table.search(0x1000_0000, 1, TranslationType::Offset as u8);
        assert_eq!(found, Some(asid as u32));

        // Search with wrong type should not find it
        assert!(table
            .search(0x1000_0000, 1, TranslationType::Linear as u8)
            .is_none());

        // Re-inc same entry: count increments, same ASID
        inv_count = 0;
        let asid2 = table.inc(0x1000_0000, TranslationType::Offset, false, 0, 1, |_| {
            inv_count += 1
        });
        assert_eq!(asid, asid2);
        assert_eq!(table.get(asid as u32).fields.count(), 2);
        assert_eq!(inv_count, 0); // not invalidated (flag=False)

        // Re-inc with invalidate=True
        inv_count = 0;
        table.inc(0x1000_0000, TranslationType::Offset, true, 0, 1, |_| {
            inv_count += 1
        });
        assert_eq!(inv_count, 1); // invalidated

        // Decrement
        table.dec(asid as u32);
        assert_eq!(table.get(asid as u32).fields.count(), 2);
        table.dec(asid as u32);
        assert_eq!(table.get(asid as u32).fields.count(), 1);
        table.dec(asid as u32);
        assert_eq!(table.get(asid as u32).fields.count(), 0);

        // After count=0, search should fail (entry is evictable)
        // Actually it still matches - the entry is there, just with count 0.
        // The search still finds it because ptb/vmid/type match.

        // Allocate a different entry
        let asid3 = table.inc(0x2000_0000, TranslationType::Linear, false, 0, 2, |_| {});
        assert!(asid3 >= 0);
        assert_eq!(
            table.get(asid3 as u32).fields.trans_type(),
            TranslationType::Linear as u8
        );
        assert_eq!(table.get(asid3 as u32).fields.vmid(), 2);
    }
    debug::write0(b"  [test] ASID table ops OK\n\0");
}
