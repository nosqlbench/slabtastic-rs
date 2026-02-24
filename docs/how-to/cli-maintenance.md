# How to Use the CLI for File Maintenance

The `slab` binary provides subcommands for inspecting, validating, and
transforming slabtastic files.

## Inspect a file

```bash
slab info data.slab
```

Displays: page count, per-page statistics (start ordinal, record count,
page size, file offset), total record count, and ordinal range.

## Check file integrity

```bash
slab check data.slab
```

Traverses every page verifying magic bytes, header/footer consistency,
page size fields, and offset array bounds.

## Retrieve records

```bash
# Human-readable hex dump
slab get data.slab 0 42 99

# Raw binary output (e.g. pipe to another tool)
slab get data.slab 0 --raw > record0.bin
```

## Append records

```bash
# From stdin (newline-delimited)
echo -e "new record 1\nnew record 2" | slab append data.slab

# From a file
slab append data.slab --source records.txt

# With custom page config
slab append data.slab --source records.txt \
    --preferred-page-size 4096 \
    --min-page-size 512 \
    --page-alignment
```

## Repack a file

Rewrite a file to new page settings, eliminating logically deleted pages
and padding waste:

```bash
slab repack input.slab output.slab \
    --preferred-page-size 65536 \
    --page-alignment
```

## Reorder records

Sort records by ordinal into a new file (useful after multiple append
cycles that may have created non-monotonic page layouts):

```bash
slab reorder input.slab sorted.slab
```
