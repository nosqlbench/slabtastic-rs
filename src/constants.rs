// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! Format constants for the slabtastic wire format.
//!
//! These constants define the structural invariants shared by all pages:
//! magic bytes, header/footer sizes, page size limits, and version tags.

/// Magic bytes identifying a slabtastic page: ASCII "SLAB".
///
/// Every page begins with these four bytes, used both for file-type
/// identification and as a structural anchor during forward traversal.
pub const MAGIC: [u8; 4] = *b"SLAB";

/// Size of the page header in bytes (4 magic + 4 page_size).
pub const HEADER_SIZE: usize = 8;

/// Minimum allowed page size in bytes (2^9).
pub const MIN_PAGE_SIZE: u32 = 512;

/// Maximum allowed page size in bytes (2^32 - 1).
pub const MAX_PAGE_SIZE: u32 = u32::MAX;

/// Size of the v1 page footer in bytes.
pub const FOOTER_V1_SIZE: usize = 16;

/// Version number for the v1 format.
pub const VERSION_1: u8 = 1;

/// Conventional file extension for slabtastic files.
///
/// By convention, slabtastic files use the `.slab` extension. This
/// constant includes the leading dot for easy use with path manipulation.
pub const SLAB_EXTENSION: &str = ".slab";

/// Page type discriminator (1-byte enum in the footer).
///
/// The page type distinguishes data pages (which hold user records) from
/// the pages page (the index). A value of 0 is reserved as invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageType {
    /// Invalid / uninitialized page (value 0). Used as a sentinel; a
    /// page with this type is always rejected during deserialization.
    Invalid = 0,
    /// Pages page — the file-level index (value 1). The last page in a
    /// valid slabtastic file is always of this type. Its records are
    /// `(start_ordinal:8, file_offset:8)` tuples sorted by ordinal.
    Pages = 1,
    /// Data page — holds user records (value 2). Records are packed
    /// contiguously and indexed by the trailing offset array.
    Data = 2,
}

impl PageType {
    /// Convert a raw byte to a `PageType`.
    pub fn from_u8(value: u8) -> Option<PageType> {
        match value {
            0 => Some(PageType::Invalid),
            1 => Some(PageType::Pages),
            2 => Some(PageType::Data),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the magic constant is the ASCII bytes "SLAB".
    #[test]
    fn test_magic_bytes() {
        assert_eq!(&MAGIC, b"SLAB");
    }

    /// Convert each valid PageType variant (0, 1, 2) to u8 and back,
    /// confirming round-trip identity. Values outside the enum (3, 255)
    /// must return `None`.
    #[test]
    fn test_page_type_roundtrip() {
        for val in 0..=2u8 {
            let pt = PageType::from_u8(val).unwrap();
            assert_eq!(pt as u8, val);
        }
        assert!(PageType::from_u8(3).is_none());
        assert!(PageType::from_u8(255).is_none());
    }
}
