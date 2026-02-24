// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! Adversarial and boundary-condition tests for the slabtastic format.
//!
//! These tests validate that the library correctly rejects malformed
//! input, handles edge cases at format boundaries, and survives
//! deliberate corruption. They complement the integration tests (which
//! exercise happy-path workflows) by targeting the error paths and
//! structural invariants described in the slabtastic design document.
//!
//! ## Categories
//!
//! - **File-level corruption** — truncated files, corrupted magic / version /
//!   page-type bytes, files that don't end with a pages page
//! - **Page deserialization** — undersized buffers, bad magic, header/footer
//!   size mismatches
//! - **Footer edge cases** — buffer too small, invalid page type (0 and 255),
//!   footer_length below minimum
//! - **Ordinal boundaries** — max positive (2^39−1), max negative (−2^39),
//!   zero, −1
//! - **Record size boundaries** — record exceeds max page capacity, record
//!   exactly fills a page
//! - **Writer config validation** — illegal size orderings, minimum below 512
//! - **Pages page** — single entry, exact match, well-past-end, negative
//!   before all entries
//! - **Alignment** — various record sizes with padding, verify on-disk
//!   multiples
//! - **Append mode** — nonexistent file, triple append
//! - **Forward traversal** — walk file from offset 0, cross-check against
//!   index
//! - **Repack / reorder round-trip** — data integrity through restructuring
//! - **PageEntry serialization** — negative ordinals, max values, zeros
//! - **Edge-case payloads** — all-empty records, single record, every byte
//!   value 0x00–0xFF

use std::io::Write;

use slabtastic::constants::{FOOTER_V1_SIZE, MAGIC};
use slabtastic::{
    Footer, Page, PageEntry, PageType, PagesPage, SlabError, SlabReader, SlabWriter, WriterConfig,
};
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// File-level corruption
// ---------------------------------------------------------------------------

/// Opening a zero-byte file must fail because there is no footer to read.
#[test]
fn test_open_empty_file() {
    let tmp = NamedTempFile::new().unwrap();
    let result = SlabReader::open(tmp.path());
    assert!(result.is_err(), "opening an empty file should fail");
}

/// Opening a one-byte file must fail — smaller than the minimum
/// structural unit (8-byte header + 16-byte footer = 24 bytes).
#[test]
fn test_open_single_byte_file() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(&[0x42]).unwrap();
    let result = SlabReader::open(tmp.path());
    assert!(result.is_err(), "opening a 1-byte file should fail");
}

/// A file containing only a valid 8-byte header (magic + page_size) but
/// no footer must be rejected. The reader needs at least header + footer
/// bytes to locate the pages page.
#[test]
fn test_open_truncated_to_header_only() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(&MAGIC).unwrap();
    tmp.write_all(&100u32.to_le_bytes()).unwrap();
    let result = SlabReader::open(tmp.path());
    assert!(result.is_err(), "file with only a header should fail");
}

/// Corrupt the first magic byte of the first data page. The reader
/// opens successfully (it reads the pages page from the end), but
/// `get(0)` must fail when it tries to deserialize the corrupted data
/// page.
#[test]
fn test_open_corrupted_magic() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let mut writer = SlabWriter::new(&path, WriterConfig::default()).unwrap();
    writer.add_record(b"test").unwrap();
    writer.finish().unwrap();

    let mut data = std::fs::read(&path).unwrap();
    data[0] = b'X';
    std::fs::write(&path, &data).unwrap();

    let mut reader = SlabReader::open(&path).unwrap();
    let result = reader.get(0);
    assert!(result.is_err(), "corrupted data page magic should cause read failure");
}

/// Corrupt the version byte (offset 13 within the footer) in the
/// trailing pages-page footer. The reader must reject the file at
/// open time because it cannot parse the pages page with an unknown
/// version.
#[test]
fn test_open_corrupted_pages_page_footer_version() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let mut writer = SlabWriter::new(&path, WriterConfig::default()).unwrap();
    writer.add_record(b"test").unwrap();
    writer.finish().unwrap();

    let mut data = std::fs::read(&path).unwrap();
    let version_pos = data.len() - FOOTER_V1_SIZE + 13;
    data[version_pos] = 99;
    std::fs::write(&path, &data).unwrap();

    let result = SlabReader::open(&path);
    assert!(result.is_err(), "corrupted version should reject file");
}

/// Change the pages-page type byte from Pages (1) to Data (2). The
/// reader must reject the file because a valid slabtastic file always
/// ends with a pages page, not a data page.
#[test]
fn test_open_corrupted_pages_page_type_to_data() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let mut writer = SlabWriter::new(&path, WriterConfig::default()).unwrap();
    writer.add_record(b"test").unwrap();
    writer.finish().unwrap();

    let mut data = std::fs::read(&path).unwrap();
    let type_pos = data.len() - FOOTER_V1_SIZE + 12;
    data[type_pos] = PageType::Data as u8;
    std::fs::write(&path, &data).unwrap();

    let result = SlabReader::open(&path);
    assert!(result.is_err(), "pages page with Data type should reject");
}

/// Manually write a single serialized data page (no pages page) to a
/// file. The reader must reject the file because the last page is
/// required to be of type Pages per the spec.
#[test]
fn test_open_file_ending_with_data_page_not_pages_page() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let mut page = Page::new(0, PageType::Data);
    page.add_record(b"hello");
    let bytes = page.serialize();
    std::fs::write(&path, &bytes).unwrap();

    let result = SlabReader::open(&path);
    assert!(result.is_err(), "file ending with data page (not pages page) should fail");
}

// ---------------------------------------------------------------------------
// Page deserialization edge cases
// ---------------------------------------------------------------------------

/// Attempting to deserialize a buffer smaller than header + footer
/// (24 bytes) must produce a `TruncatedPage` error.
#[test]
fn test_page_deserialize_truncated_buffer() {
    let tiny = vec![0u8; 10];
    let result = Page::deserialize(&tiny);
    assert!(result.is_err());
}

/// Replacing the magic bytes with "NOPE" in an otherwise valid
/// serialized page must produce an `InvalidMagic` error.
#[test]
fn test_page_deserialize_bad_magic() {
    let mut page = Page::new(0, PageType::Data);
    page.add_record(b"data");
    let mut bytes = page.serialize();
    bytes[0..4].copy_from_slice(b"NOPE");
    let result = Page::deserialize(&bytes);
    assert!(matches!(result, Err(SlabError::InvalidMagic)));
}

/// Corrupt the header's page_size field (bytes 4–7) so it disagrees
/// with the footer's page_size. Must produce a `PageSizeMismatch` error,
/// since header and footer page sizes are required to match for both
/// forward and backward traversal.
#[test]
fn test_page_deserialize_header_footer_size_mismatch() {
    let mut page = Page::new(0, PageType::Data);
    page.add_record(b"data");
    let mut bytes = page.serialize();
    let bad_size = (bytes.len() as u32 + 100).to_le_bytes();
    bytes[4..8].copy_from_slice(&bad_size);
    let result = Page::deserialize(&bytes);
    assert!(
        matches!(result, Err(SlabError::PageSizeMismatch { .. })),
        "expected PageSizeMismatch, got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Footer edge cases
// ---------------------------------------------------------------------------

/// A buffer shorter than the 16-byte v1 footer must be rejected.
#[test]
fn test_footer_buffer_too_small() {
    let buf = [0u8; 8];
    let result = Footer::read_from(&buf);
    assert!(result.is_err());
}

/// Page type 0 (Invalid) is reserved as a sentinel and must always be
/// rejected during deserialization, even though `PageType::from_u8(0)`
/// returns `Some(Invalid)`.
#[test]
fn test_footer_invalid_page_type_zero() {
    let mut buf = [0u8; FOOTER_V1_SIZE];
    let footer = Footer::new(0, 0, 512, PageType::Data);
    footer.write_to(&mut buf);
    buf[12] = 0;
    let result = Footer::read_from(&buf);
    assert!(result.is_err());
}

/// A page type byte of 255 is not a valid variant and must be rejected.
#[test]
fn test_footer_invalid_page_type_unknown() {
    let mut buf = [0u8; FOOTER_V1_SIZE];
    let footer = Footer::new(0, 0, 512, PageType::Data);
    footer.write_to(&mut buf);
    buf[12] = 255;
    let result = Footer::read_from(&buf);
    assert!(result.is_err());
}

/// A footer_length of 8 (below the 16-byte minimum) must be rejected.
/// Footer length must be at least 16 and a multiple of 16.
#[test]
fn test_footer_footer_length_too_small() {
    let mut buf = [0u8; FOOTER_V1_SIZE];
    let footer = Footer::new(0, 0, 512, PageType::Data);
    footer.write_to(&mut buf);
    buf[14..16].copy_from_slice(&8u16.to_le_bytes());
    let result = Footer::read_from(&buf);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Ordinal boundary conditions
// ---------------------------------------------------------------------------

/// The maximum positive ordinal representable in the 5-byte signed
/// format: 2^39 − 1 = 549,755,813,887. Must round-trip through
/// footer serialization without loss.
#[test]
fn test_max_positive_ordinal() {
    let max_ord: i64 = (1i64 << 39) - 1;
    let footer = Footer::new(max_ord, 1, 512, PageType::Data);
    let mut buf = [0u8; FOOTER_V1_SIZE];
    footer.write_to(&mut buf);
    let decoded = Footer::read_from(&buf).unwrap();
    assert_eq!(decoded.start_ordinal, max_ord);
}

/// The minimum (most-negative) ordinal in the 5-byte signed format:
/// −2^39 = −549,755,813,888. Must round-trip correctly — the sign
/// extension logic in `read_from` fills bytes 5–7 with 0xFF when
/// bit 39 is set.
#[test]
fn test_max_negative_ordinal() {
    let min_ord: i64 = -(1i64 << 39);
    let footer = Footer::new(min_ord, 1, 512, PageType::Data);
    let mut buf = [0u8; FOOTER_V1_SIZE];
    footer.write_to(&mut buf);
    let decoded = Footer::read_from(&buf).unwrap();
    assert_eq!(decoded.start_ordinal, min_ord);
}

/// Ordinal −1 uses the sign-extension path (bit 39 is set) and is
/// within the valid 5-byte range. Must round-trip correctly.
#[test]
fn test_ordinal_just_outside_negative_range() {
    let footer = Footer::new(-1, 0, 512, PageType::Data);
    let mut buf = [0u8; FOOTER_V1_SIZE];
    footer.write_to(&mut buf);
    let decoded = Footer::read_from(&buf).unwrap();
    assert_eq!(decoded.start_ordinal, -1);
}

/// Ordinal 0 must round-trip — the simplest non-negative case with no
/// sign extension needed.
#[test]
fn test_zero_ordinal_roundtrip() {
    let footer = Footer::new(0, 0, 512, PageType::Data);
    let mut buf = [0u8; FOOTER_V1_SIZE];
    footer.write_to(&mut buf);
    let decoded = Footer::read_from(&buf).unwrap();
    assert_eq!(decoded.start_ordinal, 0);
}

// ---------------------------------------------------------------------------
// Record size boundary conditions
// ---------------------------------------------------------------------------

/// A 500-byte record with max_page_size=512 exceeds capacity because
/// the page also needs 8 bytes of header, 8 bytes of offsets (2 × 4),
/// and 16 bytes of footer = 32 bytes of overhead. Must produce
/// `RecordTooLarge`.
#[test]
fn test_record_too_large_for_max_page() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::new(512, 512, 512, false).unwrap();
    let mut writer = SlabWriter::new(&path, config).unwrap();

    let big = vec![0u8; 500];
    let result = writer.add_record(&big);
    assert!(
        matches!(result, Err(SlabError::RecordTooLarge { .. })),
        "expected RecordTooLarge, got {result:?}"
    );
}

/// A 480-byte record exactly fills a 512-byte page (8 header + 480 data
/// + 8 offsets + 16 footer = 512). Must write and read back successfully,
/// exercising the tight-packing boundary.
#[test]
fn test_record_exactly_fits_page() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::new(512, 512, 512, false).unwrap();
    let mut writer = SlabWriter::new(&path, config).unwrap();

    let data = vec![0xAAu8; 480];
    writer.add_record(&data).unwrap();
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&path).unwrap();
    assert_eq!(reader.get(0).unwrap(), data);
}

// ---------------------------------------------------------------------------
// Writer config validation
// ---------------------------------------------------------------------------

/// min_page_size (1024) > preferred_page_size (512) violates the
/// ordering constraint and must be rejected.
#[test]
fn test_config_min_above_preferred() {
    let result = WriterConfig::new(1024, 512, 2048, false);
    assert!(result.is_err());
}

/// preferred_page_size (2048) > max_page_size (1024) violates the
/// ordering constraint and must be rejected.
#[test]
fn test_config_preferred_above_max() {
    let result = WriterConfig::new(512, 2048, 1024, false);
    assert!(result.is_err());
}

/// min_page_size (256) below the absolute minimum (512) must be rejected.
/// The spec requires all pages to be at least 512 bytes.
#[test]
fn test_config_min_below_absolute_minimum() {
    let result = WriterConfig::new(256, 512, 1024, false);
    assert!(result.is_err());
}

/// All three size parameters set to the same value (512) is a valid
/// degenerate case — every page will be exactly 512 bytes.
#[test]
fn test_config_all_equal() {
    let config = WriterConfig::new(512, 512, 512, false).unwrap();
    assert_eq!(config.min_page_size, 512);
    assert_eq!(config.preferred_page_size, 512);
    assert_eq!(config.max_page_size, 512);
}

// ---------------------------------------------------------------------------
// Pages page edge cases
// ---------------------------------------------------------------------------

/// A pages page with exactly one entry must serialize, deserialize, and
/// return the correct ordinal and offset.
#[test]
fn test_pages_page_single_entry() {
    let mut pp = PagesPage::new();
    pp.add_entry(42, 1024);
    let bytes = pp.serialize();
    let decoded = PagesPage::deserialize(&bytes).unwrap();
    assert_eq!(decoded.entry_count(), 1);
    let entries = decoded.entries();
    assert_eq!(entries[0].start_ordinal, 42);
    assert_eq!(entries[0].file_offset, 1024);
}

/// Looking up the exact start ordinal of the last entry must return
/// that entry (binary search exact-match path on the final element).
#[test]
fn test_pages_page_find_ordinal_at_exact_last_entry() {
    let mut pp = PagesPage::new();
    pp.add_entry(0, 0);
    pp.add_entry(100, 4096);
    pp.add_entry(200, 8192);

    let entry = pp.find_page_for_ordinal(200).unwrap();
    assert_eq!(entry.start_ordinal, 200);
}

/// An ordinal far beyond the last entry (999999) must still map to the
/// last page entry, since `find_page_for_ordinal` returns the greatest
/// entry ≤ the requested ordinal.
#[test]
fn test_pages_page_find_ordinal_well_past_last() {
    let mut pp = PagesPage::new();
    pp.add_entry(0, 0);
    pp.add_entry(100, 4096);

    let entry = pp.find_page_for_ordinal(999999).unwrap();
    assert_eq!(entry.start_ordinal, 100);
}

/// A negative ordinal (−1) before the first entry (0) must return
/// `None` — there is no page that could contain it.
#[test]
fn test_pages_page_negative_ordinal_before_all() {
    let mut pp = PagesPage::new();
    pp.add_entry(0, 0);

    assert!(pp.find_page_for_ordinal(-1).is_none());
}

// ---------------------------------------------------------------------------
// Alignment edge cases
// ---------------------------------------------------------------------------

/// Write records of varying sizes (1, 2, 3, and 400 bytes) with
/// alignment enabled and read them all back. Verifies that the
/// alignment padding inserted between data and offsets/footer does not
/// corrupt any record regardless of size.
#[test]
fn test_alignment_with_various_record_sizes() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::new(512, 512, u32::MAX, true).unwrap();
    let mut writer = SlabWriter::new(&path, config).unwrap();

    writer.add_record(b"a").unwrap();
    writer.add_record(b"bb").unwrap();
    writer.add_record(b"ccc").unwrap();
    writer.add_record(&vec![0xDD; 400]).unwrap();
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&path).unwrap();
    assert_eq!(reader.get(0).unwrap(), b"a");
    assert_eq!(reader.get(1).unwrap(), b"bb");
    assert_eq!(reader.get(2).unwrap(), b"ccc");
    assert_eq!(reader.get(3).unwrap(), vec![0xDD; 400]);
}

/// Write a small record with alignment enabled and verify that the
/// data page on disk occupies exactly a multiple of 512 bytes. This
/// confirms the writer's alignment padding logic produces correctly
/// sized pages.
#[test]
fn test_alignment_page_sizes_are_multiples() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::new(512, 512, u32::MAX, true).unwrap();
    let mut writer = SlabWriter::new(&path, config).unwrap();
    writer.add_record(b"short").unwrap();
    writer.finish().unwrap();

    let reader = SlabReader::open(&path).unwrap();
    let entries = reader.page_entries();
    if entries.len() == 1 {
        let file_meta = std::fs::metadata(&path).unwrap();
        let file_len = file_meta.len();
        let data = std::fs::read(&path).unwrap();
        let footer = Footer::read_from(&data[data.len() - FOOTER_V1_SIZE..]).unwrap();
        let pages_page_size = footer.page_size as u64;
        let data_page_end = file_len - pages_page_size;
        assert_eq!(
            data_page_end % 512,
            0,
            "data page size {} not aligned to 512",
            data_page_end
        );
    }
}

// ---------------------------------------------------------------------------
// Append-mode edge cases
// ---------------------------------------------------------------------------

/// Appending to a file that does not exist must fail with an I/O error.
#[test]
fn test_append_to_nonexistent_file() {
    let result = SlabWriter::append("/tmp/nonexistent_slab_file_12345.slab", WriterConfig::default());
    assert!(result.is_err());
}

/// Perform three successive appends (each adding one record) and verify
/// all four records (1 original + 3 appended) are readable with correct
/// ordinals. Exercises the append path's ordinal-continuation and
/// pages-page-rebuild logic across multiple cycles.
#[test]
fn test_multiple_appends() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::default();
    let mut w = SlabWriter::new(&path, config.clone()).unwrap();
    w.add_record(b"a").unwrap();
    w.finish().unwrap();

    for i in 1..=3u8 {
        let mut w = SlabWriter::append(&path, config.clone()).unwrap();
        w.add_record(&[b'a' + i]).unwrap();
        w.finish().unwrap();
    }

    let mut reader = SlabReader::open(&path).unwrap();
    assert_eq!(reader.get(0).unwrap(), b"a");
    assert_eq!(reader.get(1).unwrap(), b"b");
    assert_eq!(reader.get(2).unwrap(), b"c");
    assert_eq!(reader.get(3).unwrap(), b"d");
}

// ---------------------------------------------------------------------------
// Forward traversal (read_page_at_offset)
// ---------------------------------------------------------------------------

/// Write 100 records with small pages (512 B preferred) to produce
/// multiple data pages. Then walk the file from offset 0 using
/// `read_page_at_offset`, collecting each page's offset and type.
/// Verify that:
/// 1. Every index entry from the pages page appears in the forward walk.
/// 2. The last page encountered is a Pages-type page.
///
/// This exercises the `slab check` forward-traversal path that validates
/// file structure without relying on the index.
#[test]
fn test_forward_traversal_matches_index() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::new(512, 512, u32::MAX, false).unwrap();
    let mut writer = SlabWriter::new(&path, config).unwrap();
    for i in 0..100 {
        writer
            .add_record(format!("record-{i:04}").as_bytes())
            .unwrap();
    }
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&path).unwrap();
    let entries = reader.page_entries();
    let file_len = reader.file_len().unwrap();

    let mut offset: u64 = 0;
    let mut forward_offsets = Vec::new();
    while offset < file_len {
        let page = reader.read_page_at_offset(offset).unwrap();
        forward_offsets.push(offset);
        offset += page.footer.page_size as u64;
    }

    for entry in &entries {
        assert!(
            forward_offsets.contains(&(entry.file_offset as u64)),
            "index entry at offset {} not found in forward traversal",
            entry.file_offset
        );
    }

    let last_page = reader
        .read_page_at_offset(*forward_offsets.last().unwrap())
        .unwrap();
    assert_eq!(last_page.footer.page_type, PageType::Pages);
}

// ---------------------------------------------------------------------------
// Repack / reorder round-trip
// ---------------------------------------------------------------------------

/// Write 50 records with small pages (512 B) producing many pages, then
/// repack into a new file with default (64 KiB) pages. Verify every
/// record is identical in the repacked file. Exercises the `slab repack`
/// workflow: read all → write fresh with different page config.
#[test]
fn test_repack_preserves_all_records() {
    let tmp_in = NamedTempFile::new().unwrap();
    let tmp_out = NamedTempFile::new().unwrap();
    let in_path = tmp_in.path().to_path_buf();
    let out_path = tmp_out.path().to_path_buf();

    let config = WriterConfig::new(512, 512, u32::MAX, false).unwrap();
    let mut writer = SlabWriter::new(&in_path, config).unwrap();
    let records: Vec<Vec<u8>> = (0..50)
        .map(|i| format!("item-{i:04}").into_bytes())
        .collect();
    for r in &records {
        writer.add_record(r).unwrap();
    }
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&in_path).unwrap();
    let all = reader.iter().unwrap();

    let config2 = WriterConfig::default();
    let mut writer2 = SlabWriter::new(&out_path, config2).unwrap();
    for (_ord, data) in &all {
        writer2.add_record(data).unwrap();
    }
    writer2.finish().unwrap();

    let mut reader2 = SlabReader::open(&out_path).unwrap();
    for (i, expected) in records.iter().enumerate() {
        assert_eq!(reader2.get(i as i64).unwrap(), *expected);
    }
}

/// Write 20 records, read them back, sort by ordinal, and write to a
/// new file. Verify the output's ordinals are strictly monotonic. This
/// exercises the `slab reorder` workflow. (Since the normal writer
/// already produces monotonic ordinals, this mainly confirms the
/// sort-then-write pipeline doesn't introduce errors.)
#[test]
fn test_reorder_sorts_correctly() {
    let tmp_in = NamedTempFile::new().unwrap();
    let tmp_out = NamedTempFile::new().unwrap();
    let in_path = tmp_in.path().to_path_buf();
    let out_path = tmp_out.path().to_path_buf();

    let config = WriterConfig::default();
    let mut writer = SlabWriter::new(&in_path, config.clone()).unwrap();
    for i in 0..20 {
        writer
            .add_record(format!("val-{i}").as_bytes())
            .unwrap();
    }
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&in_path).unwrap();
    let mut records = reader.iter().unwrap();
    records.sort_by_key(|&(ord, _)| ord);

    let mut writer2 = SlabWriter::new(&out_path, config).unwrap();
    for (_ord, data) in &records {
        writer2.add_record(data).unwrap();
    }
    writer2.finish().unwrap();

    let mut reader2 = SlabReader::open(&out_path).unwrap();
    let all = reader2.iter().unwrap();
    for window in all.windows(2) {
        assert!(window[0].0 < window[1].0);
    }
}

// ---------------------------------------------------------------------------
// Page entry serialization
// ---------------------------------------------------------------------------

/// Round-trip a `PageEntry` with a negative ordinal and the maximum
/// `i64` file offset to exercise both ends of the value range.
#[test]
fn test_page_entry_roundtrip() {
    let entry = PageEntry {
        start_ordinal: -42,
        file_offset: i64::MAX,
    };
    let bytes = entry.to_bytes();
    let decoded = PageEntry::from_bytes(&bytes);
    assert_eq!(decoded.start_ordinal, -42);
    assert_eq!(decoded.file_offset, i64::MAX);
}

/// Round-trip a `PageEntry` with both fields set to zero — the
/// smallest valid entry.
#[test]
fn test_page_entry_zero_values() {
    let entry = PageEntry {
        start_ordinal: 0,
        file_offset: 0,
    };
    let bytes = entry.to_bytes();
    let decoded = PageEntry::from_bytes(&bytes);
    assert_eq!(decoded, entry);
}

// ---------------------------------------------------------------------------
// Edge case: file with only empty records
// ---------------------------------------------------------------------------

/// Write 100 zero-length records and read them all back. Zero-length
/// records produce consecutive identical offsets in the offset array;
/// this verifies the offset logic handles that correctly.
#[test]
fn test_file_all_empty_records() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::default();
    let mut writer = SlabWriter::new(&path, config).unwrap();
    for _ in 0..100 {
        writer.add_record(b"").unwrap();
    }
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&path).unwrap();
    for i in 0..100 {
        assert_eq!(reader.get(i).unwrap(), b"");
    }
}

// ---------------------------------------------------------------------------
// Edge case: file with exactly one record
// ---------------------------------------------------------------------------

/// Write a single record and verify: page count is 1, ordinal 0
/// returns the record, ordinals 1 and −1 produce errors, and `iter()`
/// yields exactly one `(0, data)` pair.
#[test]
fn test_single_record_file() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::default();
    let mut writer = SlabWriter::new(&path, config).unwrap();
    writer.add_record(b"only-one").unwrap();
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&path).unwrap();
    assert_eq!(reader.page_count(), 1);
    assert_eq!(reader.get(0).unwrap(), b"only-one");
    assert!(reader.get(1).is_err());
    assert!(reader.get(-1).is_err());

    let all = reader.iter().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0], (0, b"only-one".to_vec()));
}

// ---------------------------------------------------------------------------
// Edge case: binary data with all byte values
// ---------------------------------------------------------------------------

/// Store each of the 256 possible byte values (0x00–0xFF) as a
/// separate one-byte record, then read them all back. Verifies no
/// byte value is treated specially (no null-termination, no escaping,
/// no encoding issues).
#[test]
fn test_binary_data_all_byte_values() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = WriterConfig::default();
    let mut writer = SlabWriter::new(&path, config).unwrap();

    for b in 0..=255u8 {
        writer.add_record(&[b]).unwrap();
    }
    writer.finish().unwrap();

    let mut reader = SlabReader::open(&path).unwrap();
    for b in 0..=255u8 {
        assert_eq!(reader.get(b as i64).unwrap(), vec![b]);
    }
}
