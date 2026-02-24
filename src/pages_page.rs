// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! The pages page — the file-level index of a slabtastic file.
//!
//! The pages page is always the **last page** in the file. It uses the
//! standard page layout to store `PageEntry` records — tuples of
//! `(start_ordinal:8, file_offset:8)` — sorted by ordinal to support
//! O(log₂ n) binary-search lookup via [`PagesPage::find_page_for_ordinal`].
//!
//! ## Examples
//!
//! ```
//! use slabtastic::PagesPage;
//!
//! let mut pp = PagesPage::new();
//! pp.add_entry(0, 0);
//! pp.add_entry(100, 4096);
//!
//! let entry = pp.find_page_for_ordinal(50).unwrap();
//! assert_eq!(entry.start_ordinal, 0);
//! assert_eq!(entry.file_offset, 0);
//! ```

use crate::constants::PageType;
use crate::error::{Result, SlabError};
use crate::page::Page;

/// An entry in the pages page mapping a starting ordinal to a file offset.
///
/// Wire format: `[start_ordinal:8][file_offset:8]` (16 bytes, little-endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageEntry {
    /// The starting ordinal of the referenced data page.
    pub start_ordinal: i64,
    /// The byte offset of the data page within the slabtastic file.
    pub file_offset: i64,
}

impl PageEntry {
    /// Serialize this entry to 16 bytes (little-endian).
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..8].copy_from_slice(&self.start_ordinal.to_le_bytes());
        buf[8..16].copy_from_slice(&self.file_offset.to_le_bytes());
        buf
    }

    /// Deserialize an entry from 16 bytes (little-endian).
    pub fn from_bytes(buf: &[u8]) -> PageEntry {
        let start_ordinal = i64::from_le_bytes(buf[0..8].try_into().unwrap());
        let file_offset = i64::from_le_bytes(buf[8..16].try_into().unwrap());
        PageEntry {
            start_ordinal,
            file_offset,
        }
    }
}

/// A pages page (index page) that stores `PageEntry` records using the
/// standard page layout with `page_type = Pages`.
///
/// Entries are sorted by `start_ordinal` so that
/// [`find_page_for_ordinal`](Self::find_page_for_ordinal) can binary
/// search in O(log₂ n).
#[derive(Debug, Clone)]
pub struct PagesPage {
    /// The underlying page.
    pub page: Page,
}

impl PagesPage {
    /// Create a new empty pages page.
    pub fn new() -> Self {
        PagesPage {
            page: Page::new(0, PageType::Pages),
        }
    }

    /// Add an entry mapping a start ordinal to a file offset.
    pub fn add_entry(&mut self, start_ordinal: i64, file_offset: i64) {
        let entry = PageEntry {
            start_ordinal,
            file_offset,
        };
        self.page.add_record(&entry.to_bytes());
    }

    /// Parse all entries from this pages page.
    pub fn entries(&self) -> Vec<PageEntry> {
        self.page
            .records
            .iter()
            .map(|r| PageEntry::from_bytes(r))
            .collect()
    }

    /// Binary search for the page entry containing the given ordinal.
    ///
    /// Returns the entry whose `start_ordinal` is the greatest value
    /// less than or equal to `ordinal`. Entries must be sorted by
    /// `start_ordinal`.
    pub fn find_page_for_ordinal(&self, ordinal: i64) -> Option<PageEntry> {
        let entries = self.entries();
        if entries.is_empty() {
            return None;
        }

        // Binary search: find the rightmost entry where start_ordinal <= ordinal
        match entries.binary_search_by_key(&ordinal, |e| e.start_ordinal) {
            Ok(i) => Some(entries[i]),
            Err(0) => None, // ordinal is before all entries
            Err(i) => Some(entries[i - 1]),
        }
    }

    /// Serialize this pages page to bytes.
    pub fn serialize(&self) -> Vec<u8> {
        self.page.serialize()
    }

    /// Deserialize a pages page from bytes.
    pub fn deserialize(buf: &[u8]) -> Result<PagesPage> {
        let page = Page::deserialize(buf)?;
        if page.footer.page_type != PageType::Pages {
            return Err(SlabError::InvalidPageType(page.footer.page_type as u8));
        }
        Ok(PagesPage { page })
    }

    /// Return the number of page entries.
    pub fn entry_count(&self) -> usize {
        self.page.record_count()
    }
}

impl Default for PagesPage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize a pages page with 3 entries and deserialize it back.
    /// All ordinals and offsets must survive the round-trip unchanged.
    #[test]
    fn test_pages_page_roundtrip() {
        let mut pp = PagesPage::new();
        pp.add_entry(0, 0);
        pp.add_entry(100, 4096);
        pp.add_entry(200, 8192);

        let bytes = pp.serialize();
        let decoded = PagesPage::deserialize(&bytes).unwrap();
        let entries = decoded.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], PageEntry { start_ordinal: 0, file_offset: 0 });
        assert_eq!(entries[1], PageEntry { start_ordinal: 100, file_offset: 4096 });
        assert_eq!(entries[2], PageEntry { start_ordinal: 200, file_offset: 8192 });
    }

    /// Looking up an ordinal that exactly matches an entry's
    /// `start_ordinal` must return that entry (binary search
    /// exact-match path).
    #[test]
    fn test_find_page_exact() {
        let mut pp = PagesPage::new();
        pp.add_entry(0, 0);
        pp.add_entry(100, 4096);
        pp.add_entry(200, 8192);

        let entry = pp.find_page_for_ordinal(100).unwrap();
        assert_eq!(entry.start_ordinal, 100);
        assert_eq!(entry.file_offset, 4096);
    }

    /// Looking up ordinal 150, which falls between entries 100 and 200,
    /// must return the entry with start_ordinal 100 (greatest entry ≤
    /// the requested ordinal).
    #[test]
    fn test_find_page_between() {
        let mut pp = PagesPage::new();
        pp.add_entry(0, 0);
        pp.add_entry(100, 4096);
        pp.add_entry(200, 8192);

        let entry = pp.find_page_for_ordinal(150).unwrap();
        assert_eq!(entry.start_ordinal, 100);
    }

    /// Looking up ordinal 5 when the first entry starts at 10 must
    /// return `None` — no page covers ordinals before its first entry.
    #[test]
    fn test_find_page_before_first() {
        let mut pp = PagesPage::new();
        pp.add_entry(10, 0);

        assert!(pp.find_page_for_ordinal(5).is_none());
    }

    /// Looking up any ordinal in an empty pages page must return `None`.
    #[test]
    fn test_find_page_empty() {
        let pp = PagesPage::new();
        assert!(pp.find_page_for_ordinal(0).is_none());
    }
}
