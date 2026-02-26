// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! `slab export` subcommand — export content from a slab file.
//!
//! Supports text (newline-delimited), cstrings (null-terminated), and slab
//! format output. Output goes to a file or stdout.

use std::io::{self, Write};
use std::path::Path;

use crate::{SlabReader, SlabWriter};

use super::{make_writer_config, write_with_buffer_rename, ProgressReporter};

/// Run the `export` subcommand.
///
/// Exports all records from `file` in the specified format. When `output`
/// is `None`, records are written to stdout.
///
/// When `as_is` is `true`, records are written exactly as stored without
/// adding missing newlines or null terminators. By default
/// (`as_is = false`), text mode adds a trailing newline if the record
/// does not already end with one.
pub fn run(
    file: &str,
    output: Option<&str>,
    format_text: bool,
    format_cstrings: bool,
    format_slab: bool,
    as_is: bool,
    preferred_page_size: Option<u32>,
    min_page_size: Option<u32>,
    page_alignment: bool,
    progress: bool,
    namespace: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let reader = SlabReader::open_namespace(file, namespace.as_deref())?;
    let records = reader.iter()?;
    let reporter = ProgressReporter::new(progress);

    // Determine output format
    let format = if format_slab {
        ExportFormat::Slab
    } else if format_cstrings {
        ExportFormat::Cstrings
    } else if format_text {
        ExportFormat::Text
    } else if let Some(out) = output {
        detect_format_from_extension(out)
    } else {
        ExportFormat::Text
    };

    match format {
        ExportFormat::Slab => {
            let out_path = output.ok_or("slab export format requires --output")?;
            let config =
                make_writer_config(preferred_page_size, min_page_size, page_alignment)?;
            let record_count = records.len();
            write_with_buffer_rename(out_path, |buf_path| {
                let mut writer = SlabWriter::new(buf_path, config)?;
                for (_ordinal, data) in &records {
                    writer.add_record(data)?;
                    reporter.inc();
                }
                writer.finish()?;
                Ok(())
            })?;
            eprintln!("Exported {} records to {out_path} (slab)", record_count);
        }
        ExportFormat::Text => {
            let mut sink: Box<dyn Write> = match output {
                Some(path) => Box::new(std::fs::File::create(path)?),
                None => Box::new(io::stdout().lock()),
            };
            for (_ordinal, data) in &records {
                sink.write_all(data)?;
                if !as_is && !data.ends_with(b"\n") {
                    sink.write_all(b"\n")?;
                }
                reporter.inc();
            }
            sink.flush()?;
            if let Some(path) = output {
                eprintln!("Exported {} records to {path} (text)", records.len());
            }
        }
        ExportFormat::Cstrings => {
            let mut sink: Box<dyn Write> = match output {
                Some(path) => Box::new(std::fs::File::create(path)?),
                None => Box::new(io::stdout().lock()),
            };
            for (_ordinal, data) in &records {
                sink.write_all(data)?;
                if !as_is && !data.ends_with(b"\0") {
                    sink.write_all(b"\0")?;
                }
                reporter.inc();
            }
            sink.flush()?;
            if let Some(path) = output {
                eprintln!("Exported {} records to {path} (cstrings)", records.len());
            }
        }
    }

    reporter.finish();
    Ok(())
}

/// Export output format.
enum ExportFormat {
    /// Newline-delimited text.
    Text,
    /// Null-terminated binary.
    Cstrings,
    /// Slabtastic slab format.
    Slab,
}

/// Detect export format from the output file extension.
fn detect_format_from_extension(path: &str) -> ExportFormat {
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("slab") => ExportFormat::Slab,
        _ => ExportFormat::Text,
    }
}
