# CLI Reference

The `slab` binary provides file maintenance commands.

## Synopsis

```
slab <COMMAND> [OPTIONS]
```

## Commands

### `slab info <FILE>`

Display file structure and statistics: page count, per-page record
counts, page sizes, file offsets, total record count, and ordinal range.

### `slab check <FILE>`

Check a slabtastic file for structural errors. Traverses every page
verifying magic bytes, header/footer consistency, and offset bounds.

### `slab get <FILE> <ORDINALS...>`

Retrieve records by ordinal and display as hex dump.

| Flag | Description |
|------|-------------|
| `--raw` | Output raw bytes instead of hex dump |

### `slab append <FILE>`

Append newline-delimited records to an existing slabtastic file. Reads
from stdin by default.

| Flag | Description |
|------|-------------|
| `--source <PATH>` | Read records from a file instead of stdin |
| `--preferred-page-size <N>` | Preferred page size for new pages (bytes) |
| `--min-page-size <N>` | Minimum page size (bytes, >= 512) |
| `--page-alignment` | Pad new pages to multiples of min_page_size |

### `slab repack <INPUT> <OUTPUT>`

Rewrite a slabtastic file into a new file, eliminating dead pages and
applying new page configuration.

| Flag | Description |
|------|-------------|
| `--preferred-page-size <N>` | Preferred page size (bytes) |
| `--min-page-size <N>` | Minimum page size (bytes) |
| `--page-alignment` | Enable page alignment |

### `slab reorder <INPUT> <OUTPUT>`

Reorder records by ordinal into a new file. Useful after multiple append
cycles that may have produced non-monotonic page layouts.

| Flag | Description |
|------|-------------|
| `--preferred-page-size <N>` | Preferred page size (bytes) |
| `--min-page-size <N>` | Minimum page size (bytes) |
| `--page-alignment` | Enable page alignment |
