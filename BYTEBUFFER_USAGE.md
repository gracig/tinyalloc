# ByteBuffer Usage Guide

## Overview

`ByteBuffer` is a `no_std` compatible, chunk-based dynamic buffer that stores data across multiple nodes in an allocator. It provides zero-copy operations and a clean read/write API.

## Key Features

- **no_std compatible**: Works in embedded environments
- **Zero-copy operations**: `move_from` transfers ownership without copying data
- **Chunked storage**: Data is stored in CHUNK-sized nodes (8 bytes by default)
- **Max length enforcement**: Optional capacity limits
- **Iterator support**: Iterate over bytes, chunks, or ranges

## Basic Usage

### Creating a Buffer

```rust
use rsalloc::utils::bytebuffer::ByteBuffer;

// Create an unbounded buffer
let mut buf = ByteBuffer::new();

// Create a buffer with max capacity
let mut buf = ByteBuffer::with_max_len(100);
```

### Writing Data

```rust
use rsalloc::utils::backend::naive::NaiveAllocator;

let mut alloc = NaiveAllocator::new();
let mut buf = ByteBuffer::new();

// Get a writer
{
    let mut writer = buf.write(&mut alloc);
    
    // Append single byte
    writer.append(42).unwrap();
    
    // Extend with multiple bytes
    writer.extend(b"Hello World").unwrap();
}

assert_eq!(buf.len(), 12); // 1 + 11
```

### Reading Data

```rust
// Get a reader
let reader = buf.read(&alloc);

// Iterate over all bytes
for byte in reader.bytes() {
    // process byte
}

// Iterate over chunks (returns &[u8] slices)
for chunk in reader.chunks() {
    // process chunk
}

// Read a specific range
for byte in reader.range(0..5) {
    // read first 5 bytes
}
```

### Collecting Data (no_std)

Since we can't use `Vec` in `no_std`, collect into fixed arrays:

```rust
let reader = buf.read(&alloc);

// Collect into fixed array
let mut result = [0u8; 256];
let mut count = 0;
for (i, b) in reader.bytes().enumerate() {
    if i >= 256 { break; }
    result[i] = b;
    count += 1;
}

// Now result[..count] contains the data
```

### Zero-Copy Transfer

```rust
let mut src = ByteBuffer::new();
let mut dst = ByteBuffer::new();

src.write(&mut alloc).extend(b"Transfer this").unwrap();

// Zero-copy transfer (src becomes empty, dst gets the data)
dst.move_from(&mut src);

assert_eq!(src.len(), 0);
assert_eq!(dst.len(), 13);
```

### Byte-Level Copy

```rust
let mut src = ByteBuffer::new();
let mut dst = ByteBuffer::new();

src.write(&mut alloc).extend(b"Copy this").unwrap();

// Copy data (both buffers retain their data)
dst.copy_from(&mut alloc, &src).unwrap();

assert_eq!(src.len(), 9);
assert_eq!(dst.len(), 9);
```

### Capacity Management

```rust
let mut buf = ByteBuffer::with_max_len(10);

buf.write(&mut alloc).extend(b"12345").unwrap();
assert_eq!(buf.len(), 5);

// Try to exceed limit
let result = buf.write(&mut alloc).extend(b"678901");
assert!(result.is_err()); // ByteBufferError::Full

// Remove limit
buf.set_max_len(None);
buf.write(&mut alloc).extend(b"More data").unwrap();
```

### Clearing Data

```rust
let mut buf = ByteBuffer::new();
buf.write(&mut alloc).extend(b"Some data").unwrap();

// Clear all data and free allocations
buf.write(&mut alloc).clear();
assert!(buf.is_empty());
```

## NaiveAllocator for Testing

The `NaiveAllocator` provides a simple in-memory allocator for testing:

```rust
use rsalloc::utils::backend::naive::NaiveAllocator;

let mut alloc = NaiveAllocator::new();

// Allocate memory
let (handle, buf) = alloc.alloc_uninit(32).unwrap();
buf[0] = 42;

// Retrieve data
let data = alloc.get(handle).unwrap();
assert_eq!(data[0], 42);

// Free memory
alloc.free(handle);
```

### Features

- **64 slots** with up to **128 bytes** per allocation
- **Generation tracking** prevents use-after-free
- **Slot reuse** with generation increment

## Error Handling

```rust
use rsalloc::utils::bytebuffer::ByteBufferError;

match buf.write(&mut alloc).append(b'x') {
    Ok(_) => println!("Success"),
    Err(ByteBufferError::Full) => println!("Buffer full"),
    Err(ByteBufferError::AllocationFailed) => println!("Out of memory"),
    Err(ByteBufferError::Uninitialized) => println!("Invalid state"),
}
```

## Performance Notes

- Data is stored in 8-byte chunks
- Large writes (>8 bytes) span multiple nodes
- Range iteration is efficient even across chunks
- `move_from` is O(1), `copy_from` is O(n)

## Testing

Run all tests with:

```bash
cargo test --lib
```

All 18 tests cover:
- Basic append/extend operations
- Multi-chunk spanning
- Max length enforcement
- Clear operations
- Chunk iteration
- Range queries
- Move and copy operations
- Read/write lifecycle
- Empty buffer edge cases
- Large data handling
- NaiveAllocator functionality
