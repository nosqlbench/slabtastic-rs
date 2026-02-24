# Footer Format

## v1 Footer (16 bytes)

```text
Byte   Field            Width   Encoding
─────  ───────────────  ──────  ─────────────────────────────────
0–4    start_ordinal    5       signed LE, sign-extended to i64
5–7    record_count     3       unsigned LE (max 2^24 − 1)
8–11   page_size        4       unsigned LE (512 .. 2^32)
12     page_type        1       enum: 0=Invalid, 1=Pages, 2=Data
13     version          1       must be 1 for v1
14–15  footer_length    2       unsigned LE (>= 16, multiple of 16)
```

## Field details

### start_ordinal (5 bytes)

The ordinal of the first record in this page. Encoded as the low 5 bytes
of a twos-complement i64 value. On read, bit 39 is sign-extended into
bytes 5–7 to reconstruct the full i64. Range: ±2^39 (approximately
±549 billion).

### record_count (3 bytes)

The number of records in this page. Maximum: 2^24 − 1 = 16,777,215.

### page_size (4 bytes)

The total size of the page in bytes, including header, record data,
offset array, and footer. This **must** match the `page_size` field in
the header.

### page_type (1 byte)

| Value | Variant | Meaning |
|-------|---------|---------|
| 0 | Invalid | Sentinel; rejected during deserialization |
| 1 | Pages | Pages page (the file-level index) |
| 2 | Data | Data page (holds user records) |

### version (1 byte)

Format version. Must be 1 for v1 pages. A value of 0 is invalid. Readers
must reject unrecognised versions.

The footer format is **page-specific**: each page carries its own version
tag, so a single file may contain pages from different format versions
(provided every reader involved recognises all versions present).

### footer_length (2 bytes)

The total footer length in bytes. Must be at least 16 and a multiple of
16. This field enables future footer versions to extend the footer without
breaking readers that only understand v1.

## Future versions

Checksums are deferred to a future format version. Later versions may
extend the footer beyond 16 bytes by increasing `footer_length`.
Compatibility with previous readers should not be broken without explicit
user opt-in.
