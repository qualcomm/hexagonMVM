/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Simple heap allocator (tag-based free list with coalescing).
//!
//! The allocator manages a contiguous heap using inline tags. Each block has
//! a tag word containing its size (in tag-words), a `free` bit (indicating
//! the *previous* block is free), and a `released` bit (lazy deferred free).
//!
//! Layout of the heap:
//! ```text
//! [padding...] [tag0] [data...] [tag1] [data...] ... [end_tag]
//!              ^ALLOC_UNIT-1                          ^heap_size-1
//! ```
//!
//! - `tag0` at index `ALLOC_UNIT - 1`: first block tag
//! - `end_tag` at index `heap_size - 1`: sentinel with size=0, free=1
//! - Blocks are sized in multiples of `ALLOC_UNIT` tag-words
//!
//! The `free` bit in a tag indicates whether the *previous* block is free
//! (enabling O(1) backward coalescing).

use minivm_types::consts::ALLOC_UNIT;

/// Allocation tag (32-bit).
///
/// Layout: `free:1 | released:1 | size:30`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct AllocTag(pub u32);

impl AllocTag {
    pub const fn free(self) -> bool {
        self.0 & 1 != 0
    }
    pub const fn released(self) -> bool {
        (self.0 >> 1) & 1 != 0
    }
    pub const fn size(self) -> u32 {
        self.0 >> 2
    }

    pub const fn with_free(self, f: bool) -> Self {
        Self((self.0 & !1) | f as u32)
    }
    pub const fn with_released(self, r: bool) -> Self {
        Self((self.0 & !2) | ((r as u32) << 1))
    }
    pub const fn with_size(self, s: u32) -> Self {
        Self((self.0 & 0x3) | (s << 2))
    }
}

/// Allocation result: pointer + usable size.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllocBlock {
    pub offset: usize,
    pub size: usize,
}

/// Tag-based heap allocator.
///
/// Operates on a borrowed slice of `AllocTag` words.
pub struct Allocator<'a> {
    heap: &'a mut [AllocTag],
}

/// Convert byte count to tag-word count, adding one word for the tag header.
const fn words(bytes: u32) -> u32 {
    (bytes + core::mem::size_of::<AllocTag>() as u32) / core::mem::size_of::<AllocTag>() as u32
}

/// Round up to next multiple of ALLOC_UNIT (intentionally adds one extra unit
/// to ensure blocks are never zero-sized after subtracting the tag).
const fn units(word_count: u32) -> u32 {
    (word_count + ALLOC_UNIT) / ALLOC_UNIT
}

impl<'a> Allocator<'a> {
    /// Initialize a heap allocator over the given storage.
    ///
    /// `heap` must have at least `ALLOC_UNIT + 1` elements.
    /// The first usable block starts at index `ALLOC_UNIT - 1`.
    pub fn init(heap: &'a mut [AllocTag]) -> Self {
        let heap_size = heap.len() as u32;
        assert!(heap_size > ALLOC_UNIT + 1, "heap too small");

        // First block tag at index ALLOC_UNIT - 1
        let first = ALLOC_UNIT - 1;
        heap[first as usize] = AllocTag(0)
            .with_size(heap_size - ALLOC_UNIT)
            .with_free(false) // "previous" is allocated (sentinel)
            .with_released(false);

        // End sentinel at last index
        let last = heap_size - 1;
        heap[last as usize] = AllocTag(0)
            .with_size(0)
            .with_free(true) // the wilderness is free
            .with_released(false);

        Allocator { heap }
    }

    /// Free a block given its data offset (index into heap after the tag).
    ///
    /// Coalesces with adjacent free blocks.
    fn free_at(&mut self, data_offset: usize) -> usize {
        let tag_idx = data_offset - 1;
        let mut tag = self.heap[tag_idx];
        tag = tag.with_released(false);
        self.heap[tag_idx] = tag;

        let tag_size = tag.size() as usize;
        let next_idx = tag_idx + tag_size;
        let next_tag = self.heap[next_idx];

        // Try coalesce with next block
        if next_tag.size() != 0 {
            let next_next_idx = next_idx + next_tag.size() as usize;
            if next_next_idx < self.heap.len() && self.heap[next_next_idx].free() {
                // Merge with next
                let new_size = tag_size + next_tag.size() as usize;
                self.heap[tag_idx] = self.heap[tag_idx].with_size(new_size as u32);
            }
        }

        // Try coalesce with previous block
        let mut final_idx = tag_idx;
        if self.heap[tag_idx].free() {
            // Previous block is free - merge backward
            let prev_end_idx = tag_idx - 1;
            let prev_size = self.heap[prev_end_idx].size() as usize;
            let prev_idx = tag_idx - prev_size;
            let new_size = prev_size + self.heap[tag_idx].size() as usize;
            self.heap[prev_idx] = self.heap[prev_idx].with_size(new_size as u32);
            final_idx = prev_idx;
        }

        let final_tag = self.heap[final_idx];
        let final_size = final_tag.size() as usize;

        // Update the size footer at end of freed block
        if final_idx + final_size - 1 < self.heap.len() {
            self.heap[final_idx + final_size - 1] = AllocTag(0).with_size(final_size as u32);
        }

        // Mark the next block's "previous is free" bit
        if final_idx + final_size < self.heap.len() {
            self.heap[final_idx + final_size] = self.heap[final_idx + final_size].with_free(true);
        }

        final_idx
    }

    /// Allocate `request` bytes from the heap.
    ///
    /// Returns the offset (index) into the heap where data starts, and
    /// the usable size in bytes. Returns `None` if no suitable block exists.
    pub fn alloc(&mut self, request: u32) -> Option<AllocBlock> {
        let request_units = units(words(request));
        let request_words = request_units * ALLOC_UNIT;

        let mut tag_idx = ALLOC_UNIT as usize - 1;

        loop {
            let tag = self.heap[tag_idx];

            // Check if this block was deferred-freed
            if tag.released() {
                tag_idx = self.free_at(tag_idx + 1);
                continue; // Re-examine from coalesced position
            }

            let next_idx = tag_idx + tag.size() as usize;
            if next_idx >= self.heap.len() {
                return None; // Ran off end
            }

            let next_free = self.heap[next_idx].free();
            let tag_size = tag.size();

            if !next_free
                || (tag_size as usize - 1) * core::mem::size_of::<AllocTag>() < request as usize
            {
                // Block is in use or too small - skip
                tag_idx = next_idx;
                if self.heap[tag_idx].size() == 0 {
                    return None; // Reached end sentinel
                }
                continue;
            }

            // Found a free block that's big enough
            let splinter_idx = tag_idx + request_words as usize;

            if tag_size - request_words >= ALLOC_UNIT {
                // Split: create a splinter block
                let splinter_size = tag_size - request_words;
                self.heap[splinter_idx] = AllocTag(0)
                    .with_size(splinter_size)
                    .with_free(false) // previous (our block) is allocated
                    .with_released(false);

                // Update size at end of splinter
                let splinter_end = tag_idx + tag_size as usize - 1;
                self.heap[splinter_end] = AllocTag(0).with_size(splinter_size);
                // Free bit for splinter is already set by the block after it

                self.heap[tag_idx] = self.heap[tag_idx].with_size(request_words);
            } else {
                // Don't split - just mark the next block's free bit as false
                self.heap[next_idx] = self.heap[next_idx].with_free(false);
            }

            let data_offset = tag_idx + 1;
            let usable_size =
                (self.heap[tag_idx].size() as usize - 1) * core::mem::size_of::<AllocTag>();

            return Some(AllocBlock {
                offset: data_offset,
                size: usable_size,
            });
        }
    }

    /// Free a previously allocated block.
    ///
    /// `data_offset` is the offset returned by `alloc()`.
    pub fn free(&mut self, data_offset: usize) {
        self.free_at(data_offset);
    }

    /// Mark a block as released (deferred free).
    ///
    /// The block will be freed on the next allocation pass.
    pub fn release(&mut self, data_offset: usize) {
        let tag_idx = data_offset - 1;
        self.heap[tag_idx] = self.heap[tag_idx].with_released(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn make_heap(size: usize) -> Vec<AllocTag> {
        vec![AllocTag(0); size]
    }

    #[test]
    fn test_alloc_tag_fields() {
        let tag = AllocTag(0)
            .with_free(true)
            .with_released(true)
            .with_size(0x1234);
        assert!(tag.free());
        assert!(tag.released());
        assert_eq!(tag.size(), 0x1234);
    }

    #[test]
    fn test_alloc_init() {
        let mut heap = make_heap(64);
        let alloc = Allocator::init(&mut heap);

        // First tag at ALLOC_UNIT-1 should have size = heap_size - ALLOC_UNIT
        let first_tag = alloc.heap[ALLOC_UNIT as usize - 1];
        assert_eq!(first_tag.size(), 64 - ALLOC_UNIT);
        assert!(!first_tag.free());

        // End sentinel
        let end_tag = alloc.heap[63];
        assert_eq!(end_tag.size(), 0);
        assert!(end_tag.free());
    }

    #[test]
    fn test_alloc_single() {
        let mut heap = make_heap(128);
        let mut alloc = Allocator::init(&mut heap);

        let block = alloc.alloc(16).expect("allocation should succeed");
        assert!(block.size >= 16);
        assert!(block.offset > 0);
    }

    #[test]
    fn test_alloc_multiple() {
        let mut heap = make_heap(256);
        let mut alloc = Allocator::init(&mut heap);

        let b1 = alloc.alloc(8).expect("first alloc");
        let b2 = alloc.alloc(8).expect("second alloc");
        assert_ne!(b1.offset, b2.offset);
    }

    #[test]
    fn test_alloc_free_reuse() {
        let mut heap = make_heap(128);
        let mut alloc = Allocator::init(&mut heap);

        let b1 = alloc.alloc(8).expect("first alloc");
        alloc.free(b1.offset);

        let b2 = alloc.alloc(8).expect("reuse after free");
        assert_eq!(b1.offset, b2.offset); // Should reuse the freed block
    }

    #[test]
    fn test_alloc_exhaustion() {
        let mut heap = make_heap(32);
        let mut alloc = Allocator::init(&mut heap);

        // Try to allocate more than available
        let result = alloc.alloc(1024);
        assert!(result.is_none());
    }

    #[test]
    fn test_alloc_coalesce() {
        let mut heap = make_heap(256);
        let mut alloc = Allocator::init(&mut heap);

        let b1 = alloc.alloc(8).expect("first");
        let b2 = alloc.alloc(8).expect("second");
        let b3 = alloc.alloc(8).expect("third");

        // Free middle, then adjacent blocks - they should coalesce
        alloc.free(b2.offset);
        alloc.free(b1.offset);
        alloc.free(b3.offset);

        // Should be able to allocate a larger block now
        let big = alloc.alloc(b1.size as u32 + b2.size as u32 + b3.size as u32);
        assert!(big.is_some());
    }

    #[test]
    fn test_alloc_release_deferred() {
        let mut heap = make_heap(128);
        let mut alloc = Allocator::init(&mut heap);

        let b1 = alloc.alloc(8).expect("first alloc");
        alloc.release(b1.offset); // Mark as released, not freed yet

        // Next allocation should trigger deferred free and reuse
        let b2 = alloc.alloc(8).expect("reuse after release");
        assert_eq!(b1.offset, b2.offset);
    }
}
