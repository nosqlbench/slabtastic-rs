# Pages Page (Index)

The pages page is the file-level index. It is always the **last page** in
a valid slabtastic file and uses the standard page layout with
`page_type = Pages`.

## Entry format

Each record in the pages page is a 16-byte tuple:

```text
[start_ordinal:8][file_offset:8]   (little-endian signed i64)
```

- `start_ordinal` — the first ordinal of the referenced data page.
- `file_offset` — the byte offset of the data page within the file.

## Ordering

Entries are sorted by `start_ordinal` to enable O(log2 n) binary-search
lookup of any ordinal to its containing data page.

The data pages themselves are **not** required to appear in monotonic
file-offset order. After append operations, newer pages may reference
ordinal ranges that logically precede older pages on disk.

## Single-page constraint

The pages page must fit in a single page. This puts a hard upper bound on
the number of data pages in a v1 file:

```text
max_entries = (max_page_size - header - footer) / 16
```

With default `max_page_size = 2^32`, this allows over 268 million page
entries.

## Logical deletion

Data pages not referenced by the pages page are **logically deleted** and
must not be used by readers. This happens naturally in append-only mode:
when a new pages page is written, only the pages it references are live.

## Authoritative last page

A valid slabtastic file always ends with a pages page. The **last** pages
page in the file is authoritative. Earlier pages pages (from prior append
cycles) are logically dead — they remain on disk but are never consulted.

## Lookup algorithm

To find the page containing ordinal `o`:

1. Binary search the entries for the largest `start_ordinal <= o`.
2. If no such entry exists, the ordinal is in a gap (sparse) — return
   `OrdinalNotFound`.
3. Read the data page at the entry's `file_offset`.
4. Compute `local_index = o - start_ordinal`. If `local_index >=
   record_count`, the ordinal falls past the end of this page — return
   `OrdinalNotFound`.
