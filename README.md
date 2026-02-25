# Slabtastic

A streamable, random-accessible, appendable data layout format for
non-uniform data by ordinal.

```text
[magic:4 "SLAB"][page_size:4][record data...][offsets:(n+1)*4][footer:16]
```

Slabtastic stores variable-length records in self-describing **pages**,
with a trailing **pages page** that indexes every data page by ordinal.
New pages can be appended without rewriting existing data. The result is a
single `.slab` file that supports O(log n) random access, sequential
streaming, and incremental append -- with zero runtime dependencies beyond
the Rust standard library.

## Quick start

```rust
use slabtastic::{SlabWriter, SlabReader, WriterConfig};

// Write
let mut w = SlabWriter::new("demo.slab", WriterConfig::default())?;
w.add_record(b"hello")?;
w.add_record(b"world")?;
w.finish()?;

// Read
let mut r = SlabReader::open("demo.slab")?;
assert_eq!(r.get(0)?, b"hello");
assert_eq!(r.get(1)?, b"world");
```

## Features

**Reading** -- three access modes:

- **Point get** -- fetch a single record by ordinal in O(log n).
- **Batched iteration** -- `batch_iter(batch_size)` yields records in
  configurable-size batches. An empty batch signals exhaustion.
- **Sink read** -- `read_all_to_sink()` streams all records to any
  `Write` sink. `read_to_sink_async()` does the same on a background
  thread with a pollable progress handle.

**Writing** -- three write modes:

- **Single** -- `add_record()` appends one record at a time.
- **Bulk** -- `add_records()` appends a slice of records.
- **Async from iterator** -- `write_from_iter_async()` consumes an
  iterator on a background thread with pollable progress.

**Append-only** -- `SlabWriter::append()` opens an existing file and adds
new pages without modifying existing data. The old pages page is
superseded by a new one; if the append is interrupted, the file remains
valid with its original data.

**Sparse ordinals** -- ordinal ranges need not be contiguous. A file can
have gaps between pages (e.g. ordinals 0--99 and 200--299 with nothing in
between).

## CLI

The `slab` binary provides file maintenance commands:

```
slab analyze data.slab              # file structure and statistics
slab check data.slab             # structural integrity check
slab get data.slab 0 42 99       # retrieve records by ordinal
slab get data.slab 0 --raw       # raw binary output
slab get data.slab [0,10)        # ordinal range specifiers
slab get data.slab 0 --as-hex    # hex output
slab get data.slab 0 --as-base64 # base64 output
slab explain data.slab           # page layout block diagrams

# append from stdin or a file
echo -e "rec1\nrec2" | slab append data.slab
slab append data.slab --source records.txt

# import from structured formats
slab import data.slab source.json
slab import data.slab table.csv

# export to text, cstrings, or slab
slab export data.slab --output records.txt
slab export data.slab --output copy.slab

# list namespaces
slab namespaces data.slab

# rewrite with new page config (reorders + repacks)
slab rewrite input.slab output.slab --preferred-page-size 65536
```

## File layout

```
+---------------+
|  Data Page 0  |   <- offset 0
+---------------+
|  Data Page 1  |
+---------------+
|     ...       |
+---------------+
|  Data Page N  |
+---------------+
|  Pages Page   |   <- last page (single namespace); page_type = Pages
+---------------+     or Namespaces Page (multi-namespace); page_type = Namespaces
```

Each page carries a 4-byte `SLAB` magic, a 4-byte page size in both
header and footer (enabling bidirectional traversal), and a 16-byte v1
footer:

```
Byte   Field            Width
0-4    start_ordinal    5   signed LE (range +/-2^39)
5-7    record_count     3   unsigned LE (max 16,777,215)
8-11   page_size        4   unsigned LE
12     page_type        1   0=Invalid, 1=Pages, 2=Data, 3=Namespaces
13     namespace_index  1   0=invalid, 1=default, 2-127=user
14-15  footer_length    2   unsigned LE (>= 16)
```

## Page sizing

| Parameter | Default | Purpose |
|-----------|---------|---------|
| `min_page_size` | 512 | Floor / alignment boundary |
| `preferred_page_size` | 65,536 | Flush threshold |
| `max_page_size` | 2^32 - 1 | Hard ceiling |
| `page_alignment` | false | Pad to `min_page_size` multiples |

## Building

```bash
cargo build
cargo test
cargo bench --bench throughput
cargo doc --no-deps --open
```

## Documentation

Full [Diataxis](https://diataxis.fr/) documentation lives in [`docs/`](docs/index.md):

- **Tutorials** -- [Getting Started](docs/tutorials/getting-started.md),
  [Streaming I/O](docs/tutorials/streaming-io.md)
- **How-to** -- [Append Data](docs/how-to/append-data.md),
  [Import/Export](docs/how-to/import-export.md),
  [Bulk Read/Write](docs/how-to/bulk-read-write.md),
  [Async Progress](docs/how-to/async-progress.md),
  [Page Sizing](docs/how-to/page-sizing.md),
  [CLI Maintenance](docs/how-to/cli-maintenance.md)
- **Reference** -- [Wire Format](docs/reference/wire-format.md),
  [Page Layout](docs/reference/page-layout.md),
  [Footer](docs/reference/footer-format.md),
  [Pages Page](docs/reference/pages-page.md),
  [Namespaces Page](docs/reference/namespaces-page.md),
  [Errors](docs/reference/errors.md),
  [CLI](docs/reference/cli.md)
- **Explanation** -- [Why Slabtastic?](docs/explanation/why-slabtastic.md),
  [Append-Only Semantics](docs/explanation/append-only.md),
  [Sparse Ordinals](docs/explanation/sparse-ordinals.md),
  [Concurrency](docs/explanation/concurrency.md)

## Benchmarks

See [critcmp.md](critcmp.md) for throughput numbers (NVMe).

## License

Apache-2.0
