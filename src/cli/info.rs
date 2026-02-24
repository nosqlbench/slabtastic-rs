// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! `slab info` subcommand — display file structure and statistics.

use slabtastic::SlabReader;

/// Run the `info` subcommand.
pub fn run(file: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = SlabReader::open(file)?;
    let entries = reader.page_entries();

    let mut total_records: u64 = 0;
    let mut min_ordinal: Option<i64> = None;
    let mut max_ordinal: Option<i64> = None;

    println!("File: {file}");
    println!("Page count: {}", entries.len());
    println!();
    println!(
        "{:<6} {:>14} {:>10} {:>10} {:>10}",
        "Page", "Start Ordinal", "Records", "Size", "Offset"
    );
    println!("{}", "-".repeat(56));

    for (i, entry) in entries.iter().enumerate() {
        let page = reader.read_data_page(entry)?;
        let record_count = page.record_count();
        let start_ord = page.start_ordinal();
        let page_size = page.footer.page_size;

        total_records += record_count as u64;

        match min_ordinal {
            None => min_ordinal = Some(start_ord),
            Some(m) if start_ord < m => min_ordinal = Some(start_ord),
            _ => {}
        }

        let end_ord = start_ord + record_count as i64 - 1;
        match max_ordinal {
            None => max_ordinal = Some(end_ord),
            Some(m) if end_ord > m => max_ordinal = Some(end_ord),
            _ => {}
        }

        println!(
            "{:<6} {:>14} {:>10} {:>10} {:>10}",
            i, start_ord, record_count, page_size, entry.file_offset
        );
    }

    println!();
    println!("Total records: {total_records}");
    if let (Some(min), Some(max)) = (min_ordinal, max_ordinal) {
        println!("Ordinal range: {min}..={max}");
    }

    Ok(())
}
