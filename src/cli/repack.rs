// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! `slab repack` subcommand — repack a slabtastic file into a new file.

use slabtastic::{SlabReader, SlabWriter};

use super::make_writer_config;

/// Run the `repack` subcommand.
pub fn run(
    input: &str,
    output: &str,
    preferred_page_size: Option<u32>,
    min_page_size: Option<u32>,
    page_alignment: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = make_writer_config(preferred_page_size, min_page_size, page_alignment)?;

    let mut reader = SlabReader::open(input)?;
    let records = reader.iter()?;

    let mut writer = SlabWriter::new(output, config)?;
    for (_ordinal, data) in &records {
        writer.add_record(data)?;
    }
    writer.finish()?;

    println!(
        "Repacked {} records from {} to {}",
        records.len(),
        input,
        output
    );

    Ok(())
}
