// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! `slab get` subcommand — retrieve records by ordinal.

use std::io::{self, Write};

use slabtastic::SlabReader;

/// Run the `get` subcommand.
pub fn run(
    file: &str,
    ordinals: &[i64],
    raw: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = SlabReader::open(file)?;

    for &ordinal in ordinals {
        let data = reader.get(ordinal)?;

        if raw {
            io::stdout().write_all(&data)?;
        } else {
            println!("ordinal {ordinal} ({} bytes):", data.len());
            hex_dump(&data);
            println!();
        }
    }

    Ok(())
}

/// Print a hex dump of data: 16-byte lines with offset, hex, and ASCII sidebar.
fn hex_dump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let offset = i * 16;
        print!("  {offset:08x}  ");

        // Hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 {
                print!(" ");
            }
            print!("{byte:02x} ");
        }

        // Padding for short last line
        let padding = 16 - chunk.len();
        for _ in 0..padding {
            print!("   ");
        }
        if chunk.len() <= 8 {
            print!(" ");
        }

        // ASCII sidebar
        print!(" |");
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }
}
