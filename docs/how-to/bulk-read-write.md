# How to Bulk Read and Write

## Bulk write

Use `add_records()` to write multiple records in a single call:

```rust
use slabtastic::{SlabWriter, WriterConfig};

fn bulk_write(path: &str) -> slabtastic::Result<()> {
    let data: Vec<Vec<u8>> = (0..1000)
        .map(|i| format!("item-{i}").into_bytes())
        .collect();
    let refs: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();

    let mut writer = SlabWriter::new(path, WriterConfig::default())?;
    writer.add_records(&refs)?;
    writer.finish()?;
    Ok(())
}
```

`add_records` is semantically equivalent to calling `add_record` in a
loop — pages are flushed automatically as the preferred page size is
reached.

## Batched read

Use `batch_iter()` to read records in configurable batches without loading
the entire file into memory:

```rust
use slabtastic::SlabReader;

fn batched_read(path: &str) -> slabtastic::Result<()> {
    let reader = SlabReader::open(path)?;
    let mut iter = reader.batch_iter(256);

    loop {
        let batch = iter.next_batch()?;
        if batch.is_empty() {
            break;
        }
        println!("Got {} records", batch.len());
    }
    Ok(())
}
```

## Sink read

Write all records to a sink (e.g. a file or network socket) without
intermediate buffering:

```rust
use slabtastic::SlabReader;

fn sink_read(path: &str) -> slabtastic::Result<()> {
    let reader = SlabReader::open(path)?;
    let mut sink = Vec::new();
    let count = reader.read_all_to_sink(&mut sink)?;
    println!("Read {count} records ({} bytes)", sink.len());
    Ok(())
}
```

Records are written in ordinal order as raw bytes with no framing.
