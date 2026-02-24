# Concurrency Model

## Multiple readers

Multiple `SlabReader` instances can open the same file concurrently, each
with its own file descriptor. There is no shared state between readers and
no locking is required.

This is the common production pattern: several threads or processes read
the same slab file independently.

## Reader during active writes

A reader may observe an actively-written file **incrementally**, but this
is inherently optimistic:

1. The reader opens the file and reads the last pages page. At this point
   it sees whatever data pages existed when it opened.
2. As the writer appends new data pages, the reader can detect new pages
   by watching the file size grow and validating the `[magic][size]`
   header of each candidate page before reading it.
3. The reader must **not** assume atomic writes — a partially-written page
   should be skipped until its header indicates it is complete.

This mode is valid only when the writer is streaming an immutable version
of the data (no rewriting of existing pages). If the writer may rewrite
ordinal ranges, the reader should wait for the final pages page before
reading.

## Writer exclusivity

Only one writer should operate on a file at a time. There is no built-in
write locking — callers are responsible for coordinating concurrent
writers via external mechanisms (file locks, process coordination, etc.).

## Async tasks (SlabTask)

The `read_to_sink_async` and `write_from_iter_async` functions spawn a
background `std::thread`. The returned `SlabTask` handle provides:

- Thread-safe progress counters (`Arc<AtomicU64>`) polled from the caller.
- A `wait()` method that joins the background thread.

There is no async runtime dependency. The background thread owns its own
`SlabReader` or `SlabWriter` instance and file descriptor.

## Flush-at-boundaries requirement

Writers are required to flush buffers at page boundaries. The writer only
issues `write_all` calls of complete, serialized page buffers — it never
writes a partial page.

However, `write_all` does **not** guarantee OS-level atomicity. A
concurrent reader may observe a partially-written page on disk if it reads
at the exact moment the OS is flushing the write buffer. This is why
readers must validate the `[magic][size]` header of each candidate page
against the observed file size before reading the page body. A page
whose header is not yet fully visible (or whose declared size exceeds the
bytes available) should be skipped.
