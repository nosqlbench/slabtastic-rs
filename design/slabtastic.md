# Streamable, random-accessible, appendable data layout

_SLABTASTIC_, Version 1

---

After working with several formats and IO strategies for organizing non-uniform data by ordinal, it
seems we will have to make our own.

The top contenders considered were:

1) direct io, with offset table
2) arrow, with the dual-buffer paged approach
3) sqlite, with vfs offset support

Alas, none of these seem suitable.

Arrow comes close, with a meager ~15MB dependency overhead, and paged buffer layouts which support
fast access. But it comes with the dependency cost, however meager, and the limitations of not being
able to append to data without rewriting the entire file. In other words, you must write the entire
file from scratch every time. This is very not ideal.

Direct IO is a decent fit, however it requires managing multiple buffers in order to indirect
offsets, and this makes it inherently more complicated to manage for users. Having a separate offset
index file is a no-go, but you still need to have some form of it to efficiently deal with random
access and non-uniform sizing.

Sqlite is a good fit, however, it doesn't really support streaming bulk data as appendable and
incremental unless you're talking about WAL, but then you're back to juggling a directory.

# Our format - slabtastic

Our format will keep close to the metal, support optimal chunking from block stores, and keep flat
index data and values clustered close to each other. It will do this while being relatively simple,
and still support effective random IO, streaming out (batched) and streaming in values. The only
caveats will be that streaming interfaces will need to buffer and flush on boundaries that allow
page data to be written out with local buffering and flushing cadence, but this can easily be
absorbed in the readers and writer APIs.

## basic layout

This will be a large file format, supporting files of up to 2^63 bytes. All offset pointers which
target the whole file range will be twos-complement signed 8-byte integers, to facilitate easy
interop conventional stacks with simplified data types. All offsets will be little-endian.

## Pages

The major structure will be the page, which will contain, fundamentally, a set of values and a set
of flat offsets to those values.

## Page Magic

The first 4 bytes of every page will have the UTF-8 encoding of SLAB. This will be used to identify
the file as a slabtastic file. (and every page) The next 4 bytes will be the page length, which can
serve as a forward reference to the page footer, or when the page footer would be fully written by
streaming writes. This initial 8 bytes should always be considered when doing data layout.

## Page Alignment

Page alignment to the minimum page size is an optional feature, to be configured on slabtastic
writers. When enabled, pages will always be padded out to the minimum page size, and will always be
sized to be a multiple of the minimum page size. Larger page sizes will offer better utilization for
smaller minimum page sizes in this mode.

### page structure

page layout:
`[header][records][offsets][footer]`

header:
`[magic][size]`

records are simply packed data with no known structure. The offsets fully define the beginning and
end of each record, therefore there is one more offset than records, to make indexing math simple
for every record.

footer:
`[start_ordinal:5][record_count:3][page_size:4][page_type:1][version:1][footer_length:2]`

The page footer will contain, in this order:

* starting page ordinal (5-byte signed 2s complement integer)
* number of records (3-byte unsigned integer)
* page size (4-byte int)
* page type (1-byte enum value 0->invalid 1->pages page 2->data page)
* page version (1-byte int, 0->invalid, 1->v1)
* footer length (2-byte int)

The page size in the header and footer must always be equal. Checking a slabtastic file may use
these to traverse forward and backwards to verify record sizes without necessarily reading the
pages page, so long as it is focused on structure and not data as a normal user would be.

Footers are required to always be at least 16 bytes, and will be padded out to the nearest 16 bytes
in length. Checksums are deferred to a future version.

Thus, you can always read the last 16 bytes of a page to know where to find the start of the footer,
the start of the array-structured offset data, and the start of the page. And you can always read
the last 16 bytes of the file to do the same for the pages page (described below).

The v1 page footer is 16 bytes, and supports up to 2^40 in ordinal magnitude, and 2^24 in record
count per page. The beginning of the record offsets will start before the footer. The first element
location is determined by the number of records. So, from the end of the page, backup the footer
length, then -(4*(record_count + 1)). All record offsets are encoded as array-structured int offsets
from the beginning of the page, which must take account of the page header which is 4+4 bytes.

The footer format is page-specific, since you may add to a file later with an updated format. Later
versions shall not do this without the user being specifically aware that it will break
compatibility with previous readers. The initial version will be 1. A value of 0 is invalid. Readers
must verify that the page version is recognized before reading the page.

## Page Data & Sizing

Records in a page will grow from the beginning to the end of the page. Page sizes limits will be
governed by a simple heuristic: Always between 2^9 (512) and 2^32 bytes. This means that the minimum
page size will be 512 bytes, and the maximum will still easily fit within a single mmap call on
older Java systems which do not have AsyncFileChannel or similar capabilities.

Users will be able to govern page layout with some configuration parameters:

- minimum page size (must be 512 or higher)
- preferred page size (governs IO buffering behavior)
- maximum page size (must be 4GB or lower)

Of course, the size and variance of record lengths will inform user choices around these parameters.

Page alignment is enforced indirectly by the page size limitations and the requirement that all
pages be a multiple of 512 bytes. Users may prefer a page size which is larger than 512 bytes, but
this is an opportunistic setting.

When a single record exceeds the limits of a page, it is an error in v1.

## The pages page

The last page will be special. It is required to be a single page. It is a page map, which uses the
native page layout to store its values. The records in the page map are tuples of the beginning
ordinal in a page and the associated file offset within the whole slabtastic file. These are
required to be sorted by ordinal to facilitate Olog2(n) offset lookup.

The record structure for these tuples is as follows:
`[start_ordinal:8][offset:8]` (little-endian)

Even though these records will be fixed size, the layout of the pages page will not diverge from the
layout of other pages. The offsets will be encoded duplicitously in format v1. (Even though, with
the uniform size of the records, array based indexing could suffice).

The pages in the pages page are not required to be monotonically structured (aligned with the
monotonic structure of their starting ordinals.) Pages may actually be out of order, should some
append-only revisions be made to existing pages which can't or shouldn't be done in place.

Pages which are not referenced in the pages page are considered logically deleted, and should not be
used. This should happen naturally since only pages referenced in the indexing structure will be
included. Any other reader behavior which is not for slabtastic file maintenance is undefined and
should be considered a bug.

The single page requirement for the pages page puts a hard limit on the number of pages in a file,
and this is acceptable for v1.

Since the pages page has the normative page layout, the footer found at the end of the file is
sufficient as an entry point to map the whole of a slabtastic file. Essentially, opening a
slabtastic file starts with reading the last page, which is then asserted to be a pages page type
via the footer, then the ordinal offsets are read to determine the page (offset) which to jump to
next for the required ordinals (and their values).

## Append-only mode

It will be possible for pages to be logically deleted without overwriting or maintaining a delete
marker. This is done by simply by not including a page in the pages page map.

This is because a page map may "update" a previous page map with itself, and as such, it should
leave out the previous page map offsets. This can allow strictly append-only mode which does not
require overwriting a page map page, since this could be a destructive operation should it fail.

The last pages page in a slabtastic file is always the authoritative pages page. In fact, a
slabtastic file which does not end in a pages page is invalid, and therefore not slabtastic at all.

## Sparse Values

The slabtastic format will support sparse chunks. This means that ordinal values may not be fully
contiguous between the minimum and maximum ordinal values in a file. This is not a form of fine
sparseness by ordinal value, but more by chunk ranges. This affords step-wise changes to data in a
slabtastic file by simply appending new pages with ordinal holes. Although this is not strictly
necessary, it may be useful for some applications making large incremental changes.

To support sparse (coarse) structure, the APIs which are used to read ordinals from slabtastic files
MUST be able to signal that a requested ordinal is not present in the file. In such cases, consumer
APIs MAY allow the user to provide a default value to be returned, but only when this can be
explicitly requested by the user. (Simply setting it to an empty buffer is not acceptable).

Further while it may be presumed that the data in a slabtastic file is conventionally monotonic with
respect to its ordinal structure, this isn't guaranteed. As such, opportunistic readers MUST follow
the header and footer structure to verify a page is written fully before reading it. Further,
reading a slabtastic file in this way must be done with caution, and the assurance that it is an
immutable stream based on the usage scenario.

## Interior Mutation

While not the strong suite of slabtastic, it will be possible to mutate interior records in a page
so long as they are either editable in place, such as a self-terminating format (null terminated
string), or a fixed-size format (e.g. a 32-bit integer). More serious revision can also be achieved
with append-only mode by simply appending a new page map, thus providing the opportunity to rewrite
an existing page with a new one referenced in the new page map.

## File Maintenance

A slabtastic CLI will be the centralized tool for maintaining a slabtastic file. It will be
responsible for:

* analyzing a slabtastic file and give user stats and layout details
* writing a new file from an existing one to realign pages and repack data and elide unused pages
* checking a slabtastic file for errors or inconsistencies
* querying the file to extract data, given a set of ordinals
* reordering the data for monotonicity
* appending more data onto the end of a slab file

Concurrent readers streaming in a slabtastic file may incrementally read the file by watching for
updates, but this is opportunistic at best, given that revisions may occur from subsequent pages
page writes. However, as long as the reader session can safely assume the writer to be streaming _a
version_ of data which is valid, it is valid for the reader to observe the file incrementally, as
pages are written. This is a special case where mutability is not expected, and this should be made
explicit where possible. Still readers must not assume atomic writes, and thus should ensure that
the `[magic][size]` is used to determine when a page is valid for reading based on the incremental
file size.

Further, when any writers are streaming to a slabtastic file, they are required to flush buffers at
slab boundaries. This is to ensure that the pages page is always up to date with the current state
of the file across systems which do not share state via a VFS subsystem.


# Reader Interface

The reader interface should support ordinal based get, and streaming get. The streaming get
should allow the user to specify the number of records to read at a time, and the reader should
try to buffer that many. It should be possible for the reader to return less, but if the reader
returns 0 then the requestor should assume there are no more. This is the "bulk read" interface.
The reader should also be able to provide a sink for items to be read into, and a callback to be
notified when it is done, and the reader backend should the write all the data in order to the
sink provided then complete the future when it is complete. The future provided by the reader
interface should implement a decorator interface which the user can poll for status and progress.

# Writer Interface

The writer interface should support append-only mode, and streaming append. The streaming append
should allow the user to specify a block of records at a time, and the writer should buffer them
before returning. Additionally, the writer interface should be allowed to provide an iterable to
a writer with a callback, and then the writer backend should asynchronously buffer the records
as efficiently as possible, fulfilling the future when it is complete. The Future returned when
this request is made should also implement a decorator interface which the user can poll for
status and progress.

# File Naming

The filename extension for slab files shall be simply ".slab".
