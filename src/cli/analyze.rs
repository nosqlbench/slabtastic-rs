// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! `slab analyze` subcommand — display file structure and statistics.
//!
//! Displays page layout, record size statistics, page utilization, and
//! ordinal monotonicity analysis. Stats are computed by sampling — by
//! default 1000 or 1%, whichever is smaller. Override with `--samples`
//! or `--sample-percent`.

use crate::SlabReader;

/// Run the `analyze` subcommand.
pub fn run(
    file: &str,
    samples: Option<usize>,
    sample_percent: Option<f64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = SlabReader::open(file)?;
    let entries = reader.page_entries();
    let file_len = reader.file_len()?;

    let mut total_records: u64 = 0;
    let mut min_ordinal: Option<i64> = None;
    let mut max_ordinal: Option<i64> = None;

    // Collect per-page info for statistics
    let mut page_sizes: Vec<u32> = Vec::with_capacity(entries.len());
    let mut page_record_counts: Vec<usize> = Vec::with_capacity(entries.len());
    let mut page_used_bytes: Vec<usize> = Vec::with_capacity(entries.len());
    let mut record_sizes: Vec<usize> = Vec::new();
    let mut is_monotonic = true;
    let mut has_gaps = false;
    let mut prev_end_ordinal: Option<i64> = None;

    // Content detection accumulators
    let mut sampled_bytes_non_ascii = false;
    let mut sampled_bytes_has_null = false;
    let mut sampled_bytes_has_newlines = false;

    println!("File: {file}");
    println!("File size: {file_len} bytes");
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
        page_sizes.push(page_size);
        page_record_counts.push(record_count);

        // Compute used bytes (header + records + offsets + footer)
        let mut data_bytes: usize = 0;
        for r in 0..record_count {
            let rec = page.get_record(r).unwrap();
            data_bytes += rec.len();
            record_sizes.push(rec.len());

            // Content detection: check first 100 records for content hints
            if record_sizes.len() <= 100 {
                for &b in rec {
                    if !b.is_ascii() {
                        sampled_bytes_non_ascii = true;
                    }
                    if b == 0 {
                        sampled_bytes_has_null = true;
                    }
                    if b == b'\n' {
                        sampled_bytes_has_newlines = true;
                    }
                }
            }
        }
        let overhead = 8 + (record_count + 1) * 4 + 16;
        page_used_bytes.push(data_bytes + overhead);

        // Ordinal tracking
        match min_ordinal {
            None => min_ordinal = Some(start_ord),
            Some(m) if start_ord < m => min_ordinal = Some(start_ord),
            _ => {}
        }

        let end_ord = if record_count > 0 {
            start_ord + record_count as i64 - 1
        } else {
            start_ord
        };
        match max_ordinal {
            None => max_ordinal = Some(end_ord),
            Some(m) if end_ord > m => max_ordinal = Some(end_ord),
            _ => {}
        }

        // Monotonicity check
        if let Some(prev_end) = prev_end_ordinal {
            if start_ord <= prev_end {
                is_monotonic = false;
            }
            if start_ord > prev_end + 1 {
                has_gaps = true;
            }
        }
        if record_count > 0 {
            prev_end_ordinal = Some(start_ord + record_count as i64 - 1);
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

    // Ordinal monotonicity
    println!();
    if is_monotonic && !has_gaps {
        println!("Ordinal structure: strictly monotonic, no gaps");
    } else if is_monotonic && has_gaps {
        println!("Ordinal structure: monotonic with sparse gaps");
    } else {
        println!("Ordinal structure: NOT monotonic");
    }

    // Determine sampling
    let sample_count = if let Some(s) = samples {
        s
    } else if let Some(pct) = sample_percent {
        ((total_records as f64 * pct / 100.0).ceil() as usize).max(1)
    } else {
        let one_pct = (total_records as f64 * 0.01).ceil() as usize;
        one_pct.min(1000).max(1)
    };
    let actual_sample = sample_count.min(record_sizes.len());

    // Record size statistics
    if !record_sizes.is_empty() {
        let sampled = sample_evenly(&record_sizes, actual_sample);
        println!();
        println!("Record size statistics (sampled {actual_sample} of {total_records}):");
        print_stats(&sampled, "bytes");
    }

    // Page size statistics
    if !page_sizes.is_empty() {
        let ps: Vec<usize> = page_sizes.iter().map(|&s| s as usize).collect();
        println!();
        println!("Page size statistics ({} pages):", ps.len());
        print_stats(&ps, "bytes");
    }

    // Page utilization
    if !page_used_bytes.is_empty() && !page_sizes.is_empty() {
        println!();
        println!("Page utilization:");
        let mut utils: Vec<f64> = page_used_bytes
            .iter()
            .zip(page_sizes.iter())
            .map(|(&used, &total)| used as f64 / total as f64 * 100.0)
            .collect();
        utils.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let min_util = utils.first().unwrap();
        let max_util = utils.last().unwrap();
        let avg_util: f64 = utils.iter().sum::<f64>() / utils.len() as f64;
        println!("  min: {min_util:.1}%  avg: {avg_util:.1}%  max: {max_util:.1}%");
    }

    // Content type detection
    if total_records > 0 {
        let content_type = if sampled_bytes_non_ascii {
            if sampled_bytes_has_null {
                "binary (null-terminated / cstrings)"
            } else {
                "binary"
            }
        } else if sampled_bytes_has_newlines {
            "text (newline-delimited)"
        } else {
            "text"
        };
        println!();
        println!("Detected content type: {content_type}");
    }

    Ok(())
}

/// Print min/avg/max and a simple histogram for a set of values.
fn print_stats(values: &[usize], unit: &str) {
    if values.is_empty() {
        return;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let min = sorted[0];
    let max = *sorted.last().unwrap();
    let sum: usize = sorted.iter().sum();
    let avg = sum as f64 / sorted.len() as f64;

    println!("  min: {min} {unit}  avg: {avg:.1} {unit}  max: {max} {unit}");

    // Simple 5-bucket histogram
    if max > min {
        let bucket_width = ((max - min) as f64 / 5.0).ceil() as usize;
        if bucket_width > 0 {
            let mut buckets = vec![0usize; 5];
            for &v in &sorted {
                let idx = ((v - min) / bucket_width).min(4);
                buckets[idx] += 1;
            }
            println!("  histogram:");
            for (i, &count) in buckets.iter().enumerate() {
                let lo = min + i * bucket_width;
                let hi = if i == 4 { max } else { lo + bucket_width - 1 };
                let bar_len = (count * 40 / sorted.len()).max(if count > 0 { 1 } else { 0 });
                let bar = "#".repeat(bar_len);
                println!("    {lo:>8}..={hi:<8} [{count:>6}] {bar}");
            }
        }
    }
}

/// Evenly sample `n` items from `values`.
fn sample_evenly(values: &[usize], n: usize) -> Vec<usize> {
    if n >= values.len() {
        return values.to_vec();
    }
    let step = values.len() as f64 / n as f64;
    (0..n)
        .map(|i| values[(i as f64 * step) as usize])
        .collect()
}
