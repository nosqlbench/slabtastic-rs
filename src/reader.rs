// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! Slabtastic file reader.
//!
//! Opening a file reads the trailing pages page to build an in-memory
//! ordinal-to-offset index. Records can then be accessed in three modes:
//!
//! - **Point get** — [`SlabReader::get`] fetches a single record by ordinal.
//! - **Batched iteration** — [`SlabReader::batch_iter`] returns a
//!   [`SlabBatchIter`] that yields records in configurable-size batches,
//!   suitable for streaming pipelines.
//! - **Sink read** — [`SlabReader::read_all_to_sink`] writes all record
//!   data sequentially to any [`std::io::Write`] sink. For background
//!   execution with progress polling, use the associated function
//!   [`SlabReader::read_to_sink_async`].
//!
//! ## Sparse ordinals
//!
//! Ordinal ranges need not be contiguous — a file may have gaps between
//! pages (e.g. ordinals 0–99 and 200–299 with nothing in between). This
//! coarse chunk-level sparsity supports step-wise incremental changes.
//! Requesting an ordinal that falls in a gap returns
//! [`SlabError::OrdinalNotFound`].
//!
//! ## Concurrent / incremental reading
//!
//! Multiple readers may open the same file concurrently, each with its
//! own file descriptor. A reader may also observe an actively-written
//! file incrementally by validating each page's `[magic][size]` header
//! before reading it. However, the reader must not assume atomic writes;
//! pages should only be read once their header confirms they are fully
//! written. This incremental mode is inherently optimistic and should
//! only be used when the writer is streaming an immutable version of
//! the data.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::constants::{FOOTER_V1_SIZE, HEADER_SIZE, PageType};
use crate::error::{Result, SlabError};
use crate::footer::Footer;
use crate::page::Page;
use crate::pages_page::{PageEntry, PagesPage};
use crate::task::{self, SlabTask};

/// Reads slabtastic files, supporting random access by ordinal,
/// batched iteration, and streaming sink reads.
///
/// ## Read modes
///
/// - **Point get** — [`get`](Self::get) fetches a single record by ordinal.
/// - **Batched** — [`batch_iter`](Self::batch_iter) yields configurable-size
///   batches of `(ordinal, data)` pairs. An empty batch signals exhaustion.
/// - **Sink** — [`read_all_to_sink`](Self::read_all_to_sink) writes all
///   records to an [`std::io::Write`] sink. For background execution with
///   progress polling, see [`read_to_sink_async`](Self::read_to_sink_async).
///
/// ## Opening semantics
///
/// [`SlabReader::open`] reads the last 16 bytes of the file to locate
/// the pages page footer, then reads the full pages page to build the
/// index. If the file is truncated or does not end with a pages page,
/// an error is returned.
///
/// ## Sparse ordinals
///
/// Requesting an ordinal that falls in a gap between pages returns
/// [`SlabError::OrdinalNotFound`].
///
/// ## Examples
///
/// ```rust,no_run
/// use slabtastic::SlabReader;
///
/// # fn main() -> slabtastic::Result<()> {
/// let mut r = SlabReader::open("data.slab")?;
/// let record = r.get(0)?;
/// println!("record 0: {} bytes", record.len());
///
/// let all = r.iter()?;
/// for (ordinal, data) in &all {
///     println!("ordinal {ordinal}: {} bytes", data.len());
/// }
/// # Ok(())
/// # }
/// ```
pub struct SlabReader {
    file: File,
    pages_page: PagesPage,
}

impl SlabReader {
    /// Open a slabtastic file for reading.
    ///
    /// Reads the trailing pages page to build the ordinal-to-offset index.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path)?;

        let file_len = file.seek(SeekFrom::End(0))?;
        if file_len < (HEADER_SIZE + FOOTER_V1_SIZE) as u64 {
            return Err(SlabError::TruncatedPage {
                expected: HEADER_SIZE + FOOTER_V1_SIZE,
                actual: file_len as usize,
            });
        }

        // Read the last 16 bytes for the pages page footer
        file.seek(SeekFrom::End(-(FOOTER_V1_SIZE as i64)))?;
        let mut footer_buf = [0u8; FOOTER_V1_SIZE];
        file.read_exact(&mut footer_buf)?;
        let footer = Footer::read_from(&footer_buf)?;

        if footer.page_type != PageType::Pages {
            return Err(SlabError::InvalidPageType(footer.page_type as u8));
        }

        // Read the entire pages page
        let pages_page_offset = file_len - footer.page_size as u64;
        file.seek(SeekFrom::Start(pages_page_offset))?;
        let mut pages_buf = vec![0u8; footer.page_size as usize];
        file.read_exact(&mut pages_buf)?;
        let pages_page = PagesPage::deserialize(&pages_buf)?;

        Ok(SlabReader { file, pages_page })
    }

    /// Get a record by its ordinal value.
    ///
    /// Uses binary search over the pages page to locate the containing
    /// data page, then extracts the record at the local offset.
    pub fn get(&mut self, ordinal: i64) -> Result<Vec<u8>> {
        let entry = self
            .pages_page
            .find_page_for_ordinal(ordinal)
            .ok_or(SlabError::OrdinalNotFound(ordinal))?;

        let page = self.read_data_page(&entry)?;

        let local_index = (ordinal - page.start_ordinal()) as usize;
        if local_index >= page.record_count() {
            return Err(SlabError::OrdinalNotFound(ordinal));
        }

        Ok(page
            .get_record(local_index)
            .expect("index already validated")
            .to_vec())
    }

    /// Check whether the file contains a record for the given ordinal.
    pub fn contains(&mut self, ordinal: i64) -> bool {
        self.get(ordinal).is_ok()
    }

    /// Return the number of data pages in this file.
    pub fn page_count(&self) -> usize {
        self.pages_page.entry_count()
    }

    /// Return the page entries (index) from the pages page.
    pub fn page_entries(&self) -> Vec<PageEntry> {
        self.pages_page.entries()
    }

    /// Iterate all records in ordinal order, yielding `(ordinal, data)` pairs.
    pub fn iter(&mut self) -> Result<Vec<(i64, Vec<u8>)>> {
        let mut entries = self.pages_page.entries();
        // Sort by start_ordinal to ensure ordinal order
        entries.sort_by_key(|e| e.start_ordinal);

        let mut result = Vec::new();
        for entry in &entries {
            let page = self.read_data_page(entry)?;
            for i in 0..page.record_count() {
                let ordinal = page.start_ordinal() + i as i64;
                let data = page.get_record(i).unwrap().to_vec();
                result.push((ordinal, data));
            }
        }

        Ok(result)
    }

    /// Return the total file length in bytes.
    pub fn file_len(&mut self) -> Result<u64> {
        let len = self.file.seek(SeekFrom::End(0))?;
        Ok(len)
    }

    /// Read and deserialize a page starting at the given byte offset.
    ///
    /// This reads the 8-byte header to determine page size, then reads
    /// and deserializes the full page. Useful for forward traversal of
    /// the file without relying on the pages page index.
    pub fn read_page_at_offset(&mut self, offset: u64) -> Result<Page> {
        self.file.seek(SeekFrom::Start(offset))?;

        let mut hdr = [0u8; HEADER_SIZE];
        self.file.read_exact(&mut hdr)?;
        let page_size =
            u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;

        self.file.seek(SeekFrom::Start(offset))?;
        let mut page_buf = vec![0u8; page_size];
        self.file.read_exact(&mut page_buf)?;

        Page::deserialize(&page_buf)
    }

    /// Consume this reader and return a [`SlabBatchIter`] that yields
    /// records in batches of up to `batch_size`.
    ///
    /// Each call to [`SlabBatchIter::next_batch`] returns up to
    /// `batch_size` `(ordinal, data)` pairs. An empty vector signals
    /// that all records have been consumed.
    pub fn batch_iter(self, batch_size: usize) -> SlabBatchIter {
        let mut entries = self.pages_page.entries();
        entries.sort_by_key(|e| e.start_ordinal);
        SlabBatchIter {
            file: self.file,
            entries,
            batch_size,
            page_idx: 0,
            record_idx: 0,
            current_page: None,
        }
    }

    /// Write all records (in ordinal order) to `sink`, returning the
    /// number of records written.
    ///
    /// Each record's raw bytes are written directly with no framing or
    /// length prefix.
    pub fn read_all_to_sink<W: Write>(&mut self, sink: &mut W) -> Result<u64> {
        let mut entries = self.pages_page.entries();
        entries.sort_by_key(|e| e.start_ordinal);

        let mut count: u64 = 0;
        for entry in &entries {
            let page = self.read_data_page(entry)?;
            for i in 0..page.record_count() {
                let data = page.get_record(i).unwrap();
                sink.write_all(data).map_err(SlabError::from)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Spawn a background thread that reads all records from `path` and
    /// writes them to `sink`, calling `on_complete` when finished.
    ///
    /// Returns a [`SlabTask<u64>`] whose progress can be polled and
    /// whose result is the total number of records written.
    ///
    /// The `sink` and `on_complete` callback are moved into the
    /// background thread.
    pub fn read_to_sink_async<W, F>(
        path: PathBuf,
        mut sink: W,
        on_complete: F,
    ) -> SlabTask<u64>
    where
        W: Write + Send + 'static,
        F: FnOnce(u64) + Send + 'static,
    {
        let (progress, tracker) = task::new_progress();
        let handle = std::thread::spawn(move || {
            let mut reader = SlabReader::open(&path)?;
            let mut entries = reader.pages_page.entries();
            entries.sort_by_key(|e| e.start_ordinal);

            // Read all pages to compute total record count
            let mut total_records: u64 = 0;
            let mut pages: Vec<Page> = Vec::with_capacity(entries.len());
            for entry in &entries {
                let page = reader.read_data_page(entry)?;
                total_records += page.record_count() as u64;
                pages.push(page);
            }
            tracker.set_total(total_records);

            let mut count: u64 = 0;
            for page in &pages {
                for i in 0..page.record_count() {
                    let data = page.get_record(i).unwrap();
                    sink.write_all(data).map_err(SlabError::from)?;
                    count += 1;
                    tracker.inc();
                }
            }
            tracker.mark_done();
            on_complete(count);
            Ok(count)
        });
        task::new_task(handle, progress)
    }

    /// Read and deserialize a data page at the given file offset.
    pub fn read_data_page(&mut self, entry: &PageEntry) -> Result<Page> {
        self.file
            .seek(SeekFrom::Start(entry.file_offset as u64))?;

        // Read the header to get page_size
        let mut hdr = [0u8; HEADER_SIZE];
        self.file.read_exact(&mut hdr)?;
        let page_size =
            u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;

        // Read the full page
        self.file
            .seek(SeekFrom::Start(entry.file_offset as u64))?;
        let mut page_buf = vec![0u8; page_size];
        self.file.read_exact(&mut page_buf)?;

        Page::deserialize(&page_buf)
    }
}

/// Batched iterator over all records in a slabtastic file.
///
/// Created by [`SlabReader::batch_iter`]. Each call to
/// [`next_batch`](Self::next_batch) returns up to `batch_size` records
/// as `(ordinal, data)` pairs. An empty vector signals exhaustion —
/// per the design doc: "if the reader returns 0 then the requestor
/// should assume there are no more."
pub struct SlabBatchIter {
    file: File,
    entries: Vec<PageEntry>,
    batch_size: usize,
    page_idx: usize,
    record_idx: usize,
    current_page: Option<Page>,
}

impl SlabBatchIter {
    /// Return the next batch of up to `batch_size` records.
    ///
    /// Returns an empty vector when all records have been consumed.
    pub fn next_batch(&mut self) -> Result<Vec<(i64, Vec<u8>)>> {
        let mut batch = Vec::with_capacity(self.batch_size);

        while batch.len() < self.batch_size {
            // Load the current page if needed
            if self.current_page.is_none() {
                if self.page_idx >= self.entries.len() {
                    break;
                }
                let entry = self.entries[self.page_idx].clone();
                let page = self.read_data_page(&entry)?;
                self.current_page = Some(page);
                self.record_idx = 0;
            }

            let page = self.current_page.as_ref().unwrap();
            if self.record_idx >= page.record_count() {
                self.current_page = None;
                self.page_idx += 1;
                continue;
            }

            let ordinal = page.start_ordinal() + self.record_idx as i64;
            let data = page.get_record(self.record_idx).unwrap().to_vec();
            batch.push((ordinal, data));
            self.record_idx += 1;
        }

        Ok(batch)
    }

    /// Read and deserialize a data page (internal helper).
    fn read_data_page(&mut self, entry: &PageEntry) -> Result<Page> {
        self.file
            .seek(SeekFrom::Start(entry.file_offset as u64))?;

        let mut hdr = [0u8; HEADER_SIZE];
        self.file.read_exact(&mut hdr)?;
        let page_size =
            u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;

        self.file
            .seek(SeekFrom::Start(entry.file_offset as u64))?;
        let mut page_buf = vec![0u8; page_size];
        self.file.read_exact(&mut page_buf)?;

        Page::deserialize(&page_buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WriterConfig;
    use crate::writer::SlabWriter;
    use tempfile::NamedTempFile;

    /// Write 3 records, open with `SlabReader`, and verify each ordinal
    /// returns the correct data. Also confirms that requesting an
    /// ordinal beyond the last record produces an error.
    #[test]
    fn test_reader_basic() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let config = WriterConfig::default();
        let mut writer = SlabWriter::new(&path, config).unwrap();
        writer.add_record(b"alpha").unwrap();
        writer.add_record(b"beta").unwrap();
        writer.add_record(b"gamma").unwrap();
        writer.finish().unwrap();

        let mut reader = SlabReader::open(&path).unwrap();
        assert_eq!(reader.get(0).unwrap(), b"alpha");
        assert_eq!(reader.get(1).unwrap(), b"beta");
        assert_eq!(reader.get(2).unwrap(), b"gamma");
        assert!(reader.get(3).is_err());
    }

    /// Helper: write N records and return the temp path.
    fn write_test_file(n: usize) -> (tempfile::TempPath, Vec<Vec<u8>>) {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.into_temp_path();

        let config = WriterConfig::default();
        let mut writer = SlabWriter::new(&path, config).unwrap();
        let records: Vec<Vec<u8>> = (0..n)
            .map(|i| format!("rec-{i:04}").into_bytes())
            .collect();
        for rec in &records {
            writer.add_record(rec).unwrap();
        }
        writer.finish().unwrap();
        (path, records)
    }

    /// batch_iter with batch_size=1 yields one record per call.
    #[test]
    fn test_batch_iter_size_one() {
        let (path, records) = write_test_file(5);
        let reader = SlabReader::open(&path).unwrap();
        let mut iter = reader.batch_iter(1);

        for (i, expected) in records.iter().enumerate() {
            let batch = iter.next_batch().unwrap();
            assert_eq!(batch.len(), 1, "batch {i} should have 1 record");
            assert_eq!(&batch[0].1, expected);
        }
        let empty = iter.next_batch().unwrap();
        assert!(empty.is_empty(), "should be exhausted");
    }

    /// batch_iter with batch_size == total returns everything in one call.
    #[test]
    fn test_batch_iter_size_total() {
        let (path, records) = write_test_file(5);
        let reader = SlabReader::open(&path).unwrap();
        let mut iter = reader.batch_iter(5);

        let batch = iter.next_batch().unwrap();
        assert_eq!(batch.len(), 5);
        for (i, (ord, data)) in batch.iter().enumerate() {
            assert_eq!(*ord, i as i64);
            assert_eq!(data, &records[i]);
        }
        assert!(iter.next_batch().unwrap().is_empty());
    }

    /// batch_iter with batch_size > total returns all records in one call.
    #[test]
    fn test_batch_iter_size_larger_than_total() {
        let (path, records) = write_test_file(3);
        let reader = SlabReader::open(&path).unwrap();
        let mut iter = reader.batch_iter(100);

        let batch = iter.next_batch().unwrap();
        assert_eq!(batch.len(), 3);
        for (i, (_, data)) in batch.iter().enumerate() {
            assert_eq!(data, &records[i]);
        }
        assert!(iter.next_batch().unwrap().is_empty());
    }

    /// read_all_to_sink writes correct data to a Vec<u8> sink.
    #[test]
    fn test_read_all_to_sink() {
        let (path, records) = write_test_file(4);
        let mut reader = SlabReader::open(&path).unwrap();
        let mut sink: Vec<u8> = Vec::new();
        let count = reader.read_all_to_sink(&mut sink).unwrap();
        assert_eq!(count, 4);

        // Sink should contain all record bytes concatenated
        let expected: Vec<u8> = records.iter().flat_map(|r| r.iter().copied()).collect();
        assert_eq!(sink, expected);
    }
}
