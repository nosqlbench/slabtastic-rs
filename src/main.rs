// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

mod cli;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = cli::run(cli) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
