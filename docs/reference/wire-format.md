# Wire Format Specification

## Overview

A slabtastic file is a sequence of **pages** followed by a trailing
**pages page** (the index). All multi-byte integers are **little-endian**.
File-level offsets are **twos-complement signed 8-byte integers** (i64).

Files may be up to **2^63 bytes**.

The conventional file extension is **`.slab`**.

## File layout

```text
┌─────────────┐
│  Data Page 0 │  ← file offset 0
├─────────────┤
│  Data Page 1 │
├─────────────┤
│     ...      │
├─────────────┤
│  Data Page N │
├─────────────┤
│  Pages Page  │  ← always the last page; page_type = Pages
└─────────────┘
```

A valid slabtastic file **must** end with a pages page. A file that does
not end in a pages page is invalid.

## Reading entry point

1. Read the last 16 bytes of the file — this is the pages page footer.
2. Verify `page_type == Pages` and `version == 1`.
3. Compute `pages_page_offset = file_length - footer.page_size`.
4. Read the full pages page from that offset.
5. Parse the pages page entries to build an ordinal-to-offset index.

## All-integer encoding

| Width | Encoding | Usage |
|-------|----------|-------|
| 1 byte | unsigned | page_type, version |
| 2 bytes | unsigned LE | footer_length |
| 3 bytes | unsigned LE | record_count (in footer) |
| 4 bytes | unsigned LE | page_size (header and footer), record offsets |
| 5 bytes | signed LE (sign-extended) | start_ordinal (in footer) |
| 8 bytes | signed LE | file offsets, ordinals (in pages page entries) |

See also: [Page Layout](page-layout.md), [Footer Format](footer-format.md),
[Pages Page](pages-page.md).
