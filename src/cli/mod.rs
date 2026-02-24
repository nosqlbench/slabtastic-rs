// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! CLI module for the `slab` file maintenance tool.

mod append;
mod check;
mod get;
mod info;
mod reorder;
mod repack;

use clap::{Parser, Subcommand};
use slabtastic::WriterConfig;

/// slab — slabtastic file maintenance tool
#[derive(Parser)]
#[command(name = "slab", version, about = "Slabtastic file maintenance tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Available subcommands.
#[derive(Subcommand)]
pub enum Command {
    /// Display file structure and statistics.
    Info {
        /// Path to the slabtastic file.
        file: String,
    },
    /// Check a slabtastic file for structural errors.
    Check {
        /// Path to the slabtastic file.
        file: String,
    },
    /// Retrieve records by ordinal.
    Get {
        /// Path to the slabtastic file.
        file: String,
        /// Ordinals to retrieve.
        ordinals: Vec<i64>,
        /// Output raw bytes instead of hex dump.
        #[arg(long)]
        raw: bool,
    },
    /// Repack a slabtastic file into a new file.
    Repack {
        /// Input slabtastic file.
        input: String,
        /// Output slabtastic file.
        output: String,
        /// Preferred page size in bytes.
        #[arg(long)]
        preferred_page_size: Option<u32>,
        /// Minimum page size in bytes.
        #[arg(long)]
        min_page_size: Option<u32>,
        /// Enable page alignment.
        #[arg(long)]
        page_alignment: bool,
    },
    /// Append records from stdin or a source file to an existing slab file.
    Append {
        /// Path to the existing slabtastic file to append to.
        file: String,
        /// Optional source file to read records from (one per line).
        /// If omitted, reads from stdin.
        #[arg(long)]
        source: Option<String>,
        /// Preferred page size in bytes.
        #[arg(long)]
        preferred_page_size: Option<u32>,
        /// Minimum page size in bytes.
        #[arg(long)]
        min_page_size: Option<u32>,
        /// Enable page alignment.
        #[arg(long)]
        page_alignment: bool,
    },
    /// Reorder records by ordinal into a new file.
    Reorder {
        /// Input slabtastic file.
        input: String,
        /// Output slabtastic file.
        output: String,
        /// Preferred page size in bytes.
        #[arg(long)]
        preferred_page_size: Option<u32>,
        /// Minimum page size in bytes.
        #[arg(long)]
        min_page_size: Option<u32>,
        /// Enable page alignment.
        #[arg(long)]
        page_alignment: bool,
    },
}

/// Build a `WriterConfig` from optional CLI flags, falling back to defaults.
pub fn make_writer_config(
    preferred_page_size: Option<u32>,
    min_page_size: Option<u32>,
    page_alignment: bool,
) -> slabtastic::Result<WriterConfig> {
    let defaults = WriterConfig::default();
    WriterConfig::new(
        min_page_size.unwrap_or(defaults.min_page_size),
        preferred_page_size.unwrap_or(defaults.preferred_page_size),
        defaults.max_page_size,
        page_alignment,
    )
}

/// Dispatch to the appropriate subcommand.
pub fn run(cli: Cli) -> std::result::Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Info { file } => info::run(&file)?,
        Command::Check { file } => check::run(&file)?,
        Command::Get {
            file,
            ordinals,
            raw,
        } => get::run(&file, &ordinals, raw)?,
        Command::Append {
            file,
            source,
            preferred_page_size,
            min_page_size,
            page_alignment,
        } => append::run(
            &file,
            source.as_deref(),
            preferred_page_size,
            min_page_size,
            page_alignment,
        )?,
        Command::Repack {
            input,
            output,
            preferred_page_size,
            min_page_size,
            page_alignment,
        } => repack::run(
            &input,
            &output,
            preferred_page_size,
            min_page_size,
            page_alignment,
        )?,
        Command::Reorder {
            input,
            output,
            preferred_page_size,
            min_page_size,
            page_alignment,
        } => reorder::run(
            &input,
            &output,
            preferred_page_size,
            min_page_size,
            page_alignment,
        )?,
    }
    Ok(())
}
