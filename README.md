# TinyAlloc

[![Crates.io](https://img.shields.io/crates/v/tinyalloc.svg)](https://crates.io/crates/tinyalloc)
[![Documentation](https://docs.rs/tinyalloc/badge.svg)](https://docs.rs/tinyalloc)
[![License](https://img.shields.io/crates/l/tinyalloc.svg)](https://github.com/yourusername/tinyalloc#license)

A lightweight, `no_std` allocator for embedded systems with generation-tracked handles and zero-cost abstractions.

## Features

- 🚀 **Zero-cost abstractions** - Compile-time guarantees, minimal runtime overhead
- 🛡️ **Memory safety** - Generation-tracked handles prevent use-after-free
- ⚡ **O(1) operations** - Fast allocation/deallocation via free lists
- 📦 **Flexible backends** - Slab allocator included, easy to extend
- 🔧 **Dynamic buffers** - ByteBuffer and ByteBufferDeque without heap
- 🌐 **Global allocator** - Optional single-threaded global instance
- 🎯 **no_std compatible** - Works on bare metal, no dependencies

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
tinyalloc = "0.1"
```

### Basic Usage

```rust
use tinyalloc::prelude::*;

// Create allocator: 1KB memory, 32 slots
let mut alloc = TinySlabAllocator::<1024, 32>::new();

// Allocate memory
let (handle, buf) = alloc.alloc_uninit(64).unwrap();
buf.copy_from_slice(b"Hello, embedded world!");

// Access memory
let data = alloc.get(handle).unwrap();
println!("{:?}", data);

// Free memory (handle becomes invalid)
alloc.free(handle);
```

### Dynamic Buffers

```rust
use tinyalloc::prelude::*;

let mut alloc = TinySlabAllocator::<512, 16>::new();
let mut buf = ByteBuffer::new();

// Write data - automatically allocates chunks
buf.write(&mut alloc, b"Dynamic ").unwrap();
buf.write(&mut alloc, b"buffer!").unwrap();

// Read back
let data = buf.to_vec(&alloc);
assert_eq!(&data, b"Dynamic buffer!");
```

### Global Allocator

```rust
use tinyalloc::global::AllocatorConfig;
use tinyalloc::prelude::*;

fn main() {
    // Initialize once at startup
    AllocatorConfig::Slab1K32.init();
    
    // Use ByteBuffer anywhere without passing allocator
    let mut buf = ByteBuffer::new();
    buf.extend(b"Global allocator").unwrap();
}
```

### Interrupt Handler Pattern

```rust
use tinyalloc::prelude::*;
use tinyalloc::global::AllocatorConfig;

// Global queue accessible from interrupts and main
static UART_BUFFER: GlobalDeque = GlobalDeque::new();

fn uart_interrupt() {
    let byte = read_uart();
    UART_BUFFER.push(byte).ok(); // Producer
}

fn main() {
    AllocatorConfig::Slab512b16.init();
    
    loop {
        // Consumer
        if let Some(byte) = UART_BUFFER.pop() {
            process(byte);
        }
    }
}
```

## Architecture

### Core Components

```
┌─────────────────────────────────────────┐
│           Allocator Trait               │
│  (alloc, free, get, get_mut, etc.)      │
└───────────────┬─────────────────────────┘
                │
                ├──> TinySlabAllocator
                │     • Fixed-size blocks
                │     • O(1) operations
                │     • Generation tracking
                │
                └──> Custom backends
                      (implement Allocator trait)

┌─────────────────────────────────────────┐
│              ByteBuffer                 │
│  Linked list of chunks from allocator   │
└───────────────┬─────────────────────────┘
                │
                └──> ByteBufferDeque
                      • FIFO operations
                      • Auto memory reclaim
                      • Stream processing
```

### Memory Layout

**TinySlabAllocator Block:**
```
┌──────────┬─────────────────────────────┐
│ Metadata │         Data                │
│ 2 bytes  │    block_size - 2 bytes     │
└──────────┴─────────────────────────────┘
   │
   └─> [generation | length | next_slot]
       Bit-packed u16
```

### Safety Model

TinyAlloc uses **generation-tracked handles** to prevent use-after-free:

1. Each slot has a generation counter
2. Handles store both slot index and generation
3. When memory is freed, generation increments
4. Old handles become invalid automatically

```rust
let (handle, _) = alloc.alloc_uninit(64).unwrap();
alloc.free(handle); // Generation increments

// This returns None - handle is stale
assert!(alloc.get(handle).is_none());
```

## Examples

### Choosing Block Size

```rust
use tinyalloc::prelude::*;

// Option 1: Few large blocks (less fragmentation)
let alloc1 = TinySlabAllocator::<1024, 8>::new();
// Block size: 1024 / 8 = 128 bytes
// Good for: Larger allocations, lower overhead

// Option 2: Many small blocks (more flexible)
let alloc2 = TinySlabAllocator::<1024, 64>::new();
// Block size: 1024 / 64 = 16 bytes
// Good for: Many small allocations, less waste
```

### Stream Processing

```rust
use tinyalloc::prelude::*;

let mut alloc = TinySlabAllocator::<512, 16>::new();
let mut deque = ByteBufferDeque::new();

// Receive bytes (e.g., from UART)
for byte in b"packet: 0x42\n" {
    deque.push_back(&mut alloc, *byte).unwrap();
}

// Parse packet
let mut packet = Vec::new();
while let Some(byte) = deque.pop_front(&alloc) {
    if byte == b'\n' { break; }
    packet.push(byte);
}
```

### Error Handling

```rust
use tinyalloc::prelude::*;

let mut alloc = TinySlabAllocator::<256, 4>::new(); // Very small
let mut buf = ByteBuffer::new();

// Handle allocation failures
match buf.write(&mut alloc, b"some data") {
    Ok(_) => println!("Success"),
    Err(ByteBufferError::AllocationFailed) => {
        println!("Out of memory!");
    }
    Err(e) => println!("Error: {:?}", e),
}
```

## Feature Flags

```toml
[dependencies.tinyalloc]
version = "0.1"
default-features = false
features = ["tinyslab", "bytebuffer"]
```

- **`tinyslab`** (default) - Slab allocator implementation
- **`bytebuffer`** (default) - Dynamic byte buffer
- **`bytebuffer-deque`** (default) - FIFO queue implementation
- **`global-alloc`** (default) - Single-threaded global allocator
- **`global-alloc-custom`** - Custom-sized global allocators

## Performance

| Operation | TinySlabAllocator | Notes |
|-----------|------------------|-------|
| `alloc()` | O(1) | Free list lookup |
| `free()` | O(1) | Add to free list |
| `get()` | O(1) | Direct array access |
| Memory overhead | 3 bytes/slot | Metadata per slot |
| Block overhead | 2 bytes | Metadata per block |

## Limitations

- **Maximum 255 slots** - Uses u8 for slot indices
- **Single-threaded** - No built-in synchronization
- **Fixed block size** - All blocks same size per allocator
- **No defragmentation** - Freed blocks don't coalesce

## Testing

Run tests with single thread (due to global allocator):

```bash
cargo test --all-features -- --test-threads=1
```

## Platform Support

TinyAlloc is `no_std` and works on:

- ✅ ARM Cortex-M (embedded)
- ✅ RISC-V
- ✅ AVR
- ✅ x86/x86_64 (hosted)
- ✅ Any platform with Rust support

## Comparison

| Feature | TinyAlloc | linked_list_allocator | embedded-alloc |
|---------|-----------|----------------------|----------------|
| no_std | ✅ | ✅ | ✅ |
| Generation tracking | ✅ | ❌ | ❌ |
| O(1) alloc | ✅ | ❌ (O(n)) | ✅ |
| Dynamic buffers | ✅ | ❌ | ❌ |
| Global allocator | ✅ | ✅ | ✅ |
| Zero dependencies | ✅ | ❌ | ✅ |

## Contributing

Contributions are welcome! Please:

1. Check existing issues
2. Add tests for new features
3. Run `cargo clippy --all-features`
4. Run `cargo test --all-features -- --test-threads=1`

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Resources

- [Documentation](https://docs.rs/tinyalloc)
- [Repository](https://github.com/yourusername/tinyalloc)
- [Examples](examples/)
- [Changelog](CHANGELOG.md)

---

Made with ❤️ for embedded systems
