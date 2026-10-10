/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! L2 cache initialization.
//!
//! Computes L2 cache size and tag configuration from hardware
//! config table values or built-in architecture tables.

/// L2 chunk size for small arrays (bytes).
pub const L2_CHUNK: u32 = 32 * 1024; // 32 KB

/// L2 chunk size for large arrays (bytes).
pub const L2_BIG_CHUNK: u32 = 64 * 1024; // 64 KB

/// Threshold where chunk size switches from L2_CHUNK to L2_BIG_CHUNK.
pub const L2_CHUNK_SWITCH: u32 = 32;

/// L2 QoS watermark LO.
pub const L2_QOS_WATERMARK_LO: u32 = 24;

/// L2 QoS watermark L2.
pub const L2_QOS_WATERMARK_L2: u32 = 24;

/// L2 QoS mode TL.
pub const L2_QOS_MODE_TL: u32 = 1;

/// SYSCFG L2CFG "max cache" encoding.
pub const SYSCFG_L2CFG_MAX: u32 = 7;

/// L2 tag size encodings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum L2TagSize {
    S0 = 0,
    S64 = 1,
    S128 = 2,
    S256 = 3,
    S512 = 4,
    S1024 = 5,
    Reserved = 7,
}

/// Calculate L2 cache size in bytes from array size encoding.
///
/// For v65 and earlier, the L2 array size field maps to actual size
/// using a chunked scheme:
/// - Arrays 0..=32: size = l2arr * L2_CHUNK (32 KB per unit)
/// - Arrays 33+: size = 32*L2_CHUNK + (l2arr - 32)*L2_BIG_CHUNK
pub fn l2_size_from_array(l2arr: u32) -> u32 {
    if l2arr > L2_CHUNK_SWITCH {
        L2_CHUNK_SWITCH * L2_CHUNK + (l2arr - L2_CHUNK_SWITCH) * L2_BIG_CHUNK
    } else {
        l2arr * L2_CHUNK
    }
}

/// Calculate L2 cache size for v68+ from config table array_size.
///
/// The config table provides size in KB directly.
pub const fn l2_size_from_config_kb(array_size_kb: u32) -> u32 {
    array_size_kb * 1024
}

/// Convert a raw tag size (from config table) to SYSCFG register encoding.
///
/// Special case: 1536 maps to SYSCFG_L2CFG_MAX (max cache).
/// Otherwise: encoding = count_trailing_zeros(tag_size) - 5
pub fn tag_size_to_syscfg(tag_size: u32) -> u32 {
    if tag_size == 1536 {
        SYSCFG_L2CFG_MAX
    } else if tag_size == 0 {
        0
    } else {
        let tz = tag_size.trailing_zeros();
        tz.saturating_sub(5)
    }
}

/// Result of L2 cache initialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct L2CacheConfig {
    /// L2 size in bytes.
    pub l2size: u32,
    /// Tag size encoding for SYSCFG register.
    pub l2tags: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_size_small() {
        assert_eq!(l2_size_from_array(0), 0);
        assert_eq!(l2_size_from_array(1), 32 * 1024);
        assert_eq!(l2_size_from_array(8), 256 * 1024); // 256 KB
        assert_eq!(l2_size_from_array(16), 512 * 1024); // 512 KB
        assert_eq!(l2_size_from_array(32), 1024 * 1024); // 1 MB
    }

    #[test]
    fn test_l2_size_large() {
        // 33 units: 32*32K + 1*64K = 1024K + 64K = 1088K
        assert_eq!(l2_size_from_array(33), 1024 * 1024 + 64 * 1024);
        // 48 units: 32*32K + 16*64K = 1024K + 1024K = 2048K
        assert_eq!(l2_size_from_array(48), 2 * 1024 * 1024);
    }

    #[test]
    fn test_l2_size_from_config() {
        assert_eq!(l2_size_from_config_kb(512), 512 * 1024);
        assert_eq!(l2_size_from_config_kb(1024), 1024 * 1024);
    }

    #[test]
    fn test_tag_size_to_syscfg() {
        // 1536 → max cache
        assert_eq!(tag_size_to_syscfg(1536), SYSCFG_L2CFG_MAX);
        // 0 → 0
        assert_eq!(tag_size_to_syscfg(0), 0);
        // 64 = 2^6, trailing_zeros = 6, 6 - 5 = 1
        assert_eq!(tag_size_to_syscfg(64), 1);
        // 128 = 2^7, trailing_zeros = 7, 7 - 5 = 2
        assert_eq!(tag_size_to_syscfg(128), 2);
        // 256 = 2^8 → 3
        assert_eq!(tag_size_to_syscfg(256), 3);
        // 512 = 2^9 → 4
        assert_eq!(tag_size_to_syscfg(512), 4);
        // 1024 = 2^10 → 5
        assert_eq!(tag_size_to_syscfg(1024), 5);
    }

    #[test]
    fn test_l2_cache_config() {
        let cfg = L2CacheConfig {
            l2size: 1024 * 1024,
            l2tags: 5,
        };
        assert_eq!(cfg.l2size, 1024 * 1024);
        assert_eq!(cfg.l2tags, 5);
    }
}
