// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! `slab reorder` subcommand — reorder records by ordinal into a new file.

use slabtastic::{SlabReader, SlabWriter};

use super::make_writer_config;

/// Run the `reorder` subcommand.
pub fn run(
    input: &str,
    output: &str,
    preferred_page_size: Option<u32>,
    min_page_size: Option<u32>,
    page_alignment: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = make_writer_config(preferred_page_size, min_page_size, page_alignment)?;

    let mut reader = SlabReader::open(input)?;
    let mut records = reader.iter()?;

    // Check if already monotonic
    let already_monotonic = records
        .windows(2)
        .all(|w| w[0].0 <= w[1].0);

    // Sort by ordinal
    records.sort_by_key(|&(ordinal, _)| ordinal);

    let mut writer = SlabWriter::new(output, config)?;
    for (_ordinal, data) in &records {
        writer.add_record(data)?;
    }
    writer.finish()?;

    if already_monotonic {
        println!("Input was already monotonically ordered.");
    } else {
        println!("Input was NOT monotonically ordered; records have been sorted.");
    }

    println!(
        "Reordered {} records from {} to {}",
        records.len(),
        input,
        output
    );

    Ok(())
}
