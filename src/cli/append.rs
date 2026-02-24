// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! `slab append` subcommand — append data to an existing slabtastic file.
//!
//! Reads newline-delimited records from stdin (or a source file) and
//! appends them to an existing slab file via [`SlabWriter::append`].

use std::io::{self, BufRead};

use slabtastic::SlabWriter;

use super::make_writer_config;

/// Run the `append` subcommand.
///
/// Reads newline-delimited records from `source` (or stdin if `None`)
/// and appends them to the existing slab file at `file`.
pub fn run(
    file: &str,
    source: Option<&str>,
    preferred_page_size: Option<u32>,
    min_page_size: Option<u32>,
    page_alignment: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = make_writer_config(preferred_page_size, min_page_size, page_alignment)?;
    let mut writer = SlabWriter::append(file, config)?;

    let mut count: u64 = 0;

    match source {
        Some(path) => {
            let file = std::fs::File::open(path)?;
            let reader = io::BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                writer.add_record(line.as_bytes())?;
                count += 1;
            }
        }
        None => {
            let stdin = io::stdin();
            let reader = stdin.lock();
            for line in reader.lines() {
                let line = line?;
                writer.add_record(line.as_bytes())?;
                count += 1;
            }
        }
    }

    writer.finish()?;
    println!("Appended {count} records to {file}");
    Ok(())
}
