# Error Catalogue

All fallible library functions return `Result<T, SlabError>`.

| Variant | Meaning | Common cause |
|---------|---------|--------------|
| `InvalidMagic` | First 4 bytes are not `SLAB` | Corrupt file, wrong file type |
| `InvalidVersion(u8)` | Footer version is not recognised | Future format version, corruption |
| `InvalidPageType(u8)` | Page type byte is not 0, 1, or 2 | Corruption, invalid file |
| `PageSizeMismatch { header, footer }` | Header and footer page_size differ | Truncation, in-place corruption |
| `PageTooSmall(u32)` | Configured page size < 512 | Invalid `WriterConfig` |
| `PageTooLarge(u64)` | Configured page size > 2^32 | Invalid `WriterConfig` |
| `RecordTooLarge { record_size, max_size }` | Single record exceeds page capacity | Record too big for `max_page_size` |
| `OrdinalNotFound(i64)` | Requested ordinal is not in the file | Sparse gap, out-of-range lookup |
| `InvalidFooter(String)` | Footer data is malformed | Corruption, bad footer_length |
| `TruncatedPage { expected, actual }` | Page data is incomplete | Truncated file, partial write |
| `Io(io::Error)` | Underlying I/O error | File not found, permission denied |
