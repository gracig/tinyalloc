//! # TinyAlloc - A no_std Allocator for Embedded Systems
//!
//! TinyAlloc provides lightweight, predictable memory allocation for embedded and no_std environments.
//! It features:
//!
//! - **Zero-cost abstractions** - Compile-time guarantees with minimal runtime overhead
//! - **Generation-tracked handles** - Safe memory access with automatic invalidation
//! - **Configurable backends** - Choose the allocator that fits your constraints
//! - **ByteBuffer utilities** - Efficient dynamic buffers without heap allocation
//! - **Global allocator support** - Optional single-threaded global allocator
//!
//! ## Quick Start
//!
//! ```rust
//! use tinyalloc::prelude::*;
//!
//! // Create a slab allocator with 1KB memory and 32 slots
//! let mut alloc = TinySlabAllocator::<1024, 32>::new();
//!
//! // Create a dynamic buffer
//! let mut buffer = ByteBuffer::new();
//! buffer.write(&mut alloc, b"Hello, embedded world!").unwrap();
//!
//! // Read data back
//! let data = buffer.to_vec(&alloc);
//! assert_eq!(&data, b"Hello, embedded world!");
//! ```
//!
//! ## Features
//!
//! - `tinyslab` (default) - Slab allocator with generation tracking
//! - `bytebuffer` (default) - Dynamic byte buffer backed by allocators
//! - `bytebuffer-deque` (default) - Double-ended queue for stream processing
//! - `global-alloc` (default) - Single-threaded global allocator
//! - `global-alloc-custom` - Custom-sized global allocators
//!
//! ## Architecture
//!
//! ### Core Traits
//!
//! The [`Allocator`] trait defines the interface for all allocator backends:
//!
//! - `alloc()` / `alloc_uninit()` - Allocate memory blocks
//! - `get()` / `get_mut()` - Access allocated memory via handles
//! - `free()` - Release memory blocks
//! - `len()` / `capacity()` - Query allocator state
//!
//! ### Safety Model
//!
//! TinyAlloc uses **generation-tracked handles** ([`Handle`]) to prevent use-after-free:
//!
//! ```rust
//! # use tinyalloc::prelude::*;
//! # let mut alloc = TinySlabAllocator::<1024, 32>::new();
//! let (handle, buf) = alloc.alloc_uninit(64).unwrap();
//! buf.copy_from_slice(b"data");
//!
//! alloc.free(handle); // Increments generation
//!
//! // This returns None - handle is invalidated
//! assert!(alloc.get(handle).is_none());
//! ```
//!
//! ### Backends
//!
//! **TinySlabAllocator** - Fixed-size blocks with O(1) allocation:
//! - Const generics for compile-time configuration
//! - Maximum 255 slots (u8 metadata)
//! - Bit-packed metadata (2 bytes per block)
//!
//! ### Utilities
//!
//! **ByteBuffer** - Dynamic buffer without heap allocation:
//! - Linked-list of chunks from allocator
//! - Zero-copy operations where possible
//! - Optional maximum length enforcement
//!
//! **ByteBufferDeque** - Stream processing:
//! - Push to back, pop from front
//! - Automatic memory reclamation
//! - Perfect for UART/SPI buffers
//!
//! **GlobalDeque** - Single-threaded global access:
//! - Static lifetime for interrupt handlers
//! - No mutex overhead
//! - Ideal for producer/consumer patterns
//!
//! ## Examples
//!
//! ### Basic Allocation
//!
//! ```rust
//! use tinyalloc::prelude::*;
//!
//! let mut alloc = TinySlabAllocator::<512, 16>::new();
//!
//! // Allocate and write
//! let handle = alloc.alloc(b"test").unwrap();
//!
//! // Read back
//! let data = alloc.get(handle).unwrap();
//! assert_eq!(data, b"test");
//!
//! // Free memory
//! alloc.free(handle);
//! ```
//!
//! ### Global Allocator Pattern
//!
//! ```rust,no_run
//! use tinyalloc::global::AllocatorConfig;
//! use tinyalloc::prelude::*;
//!
//! fn main() {
//!     // Initialize once at startup
//!     AllocatorConfig::Slab1K32.init();
//!
//!     // Use ByteBuffer anywhere
//!     let mut buf = ByteBuffer::new();
//!     buf.extend(b"Global allocator!").unwrap();
//! }
//! ```
//!
//! ### Interrupt Handler Pattern
//!
//! ```rust,no_run
//! use tinyalloc::prelude::*;
//! use tinyalloc::global::AllocatorConfig;
//!
//! static UART_BUFFER: GlobalDeque = GlobalDeque::new();
//!
//! fn interrupt_handler() {
//!     let byte = read_uart();
//!     UART_BUFFER.push(byte).ok();
//! }
//!
//! fn main_loop() {
//!     AllocatorConfig::Slab512b16.init();
//!     
//!     loop {
//!         if let Some(byte) = UART_BUFFER.pop() {
//!             process(byte);
//!         }
//!     }
//! }
//! # fn read_uart() -> u8 { 0 }
//! # fn process(_: u8) {}
//! ```
//!
//! ## Performance Characteristics
//!
//! | Operation | TinySlabAllocator | Notes |
//! |-----------|------------------|-------|
//! | alloc() | O(1) | Free list lookup |
//! | free() | O(1) | Add to free list |
//! | get() | O(1) | Direct array access |
//! | Memory | 2 bytes/block | Bit-packed metadata |
//!
//! ## Safety Guarantees
//!
//! - ✅ **No unsafe in public API** - Safe Rust interface
//! - ✅ **Generation tracking** - Prevents use-after-free
//! - ✅ **Bounds checking** - All accesses validated
//! - ✅ **No panics** - Returns `Option`/`Result` for errors
//! - ✅ **Single-threaded** - Clear concurrency model
//!
//! ## no_std Support
//!
//! TinyAlloc is fully `no_std` compatible:
//! - Zero dependencies
//! - No heap allocation required
//! - Works on bare metal
//! - Supports embedded platforms
//!
//! ## Testing
//!
//! Due to the single-threaded global allocator:
//!
//! ```bash
//! cargo test --all-features -- --test-threads=1
//! ```
//!
//! ## License
//!
//! This project is licensed under your chosen license.

#![no_std]
#![warn(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tinyalloc/0.1.0")]

// Internal modules - use prelude for public API
mod backend;
mod utils;

#[cfg(feature = "global-alloc")]
mod global;

/// Convenient re-exports for common use - this is the public API
///
/// Users should import from prelude:
/// ```
/// use tinyalloc::prelude::*;
/// ```
pub mod prelude;

/// Returns the version string of the tinyalloc crate
///
/// # Examples
///
/// ```
/// use tinyalloc::version;
/// assert_eq!(version(), "0.1.0");
/// ```
pub fn version() -> &'static str {
    "0.1.0"
}

/// Defines bit widths for packing metadata into a u16
///
/// This layout is determined by the allocator based on its constraints:
/// - `slot_bits`: Bits needed to represent slot indices (log2 of max slots)
/// - `gen_bits`: Bits for generation counter (typically 4-8 bits)
/// - `len_bits`: Bits for data length (remaining bits after slot + gen)
///
/// Total: slot_bits + gen_bits + len_bits = 16 bits
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitLayout {
    /// Number of bits for slot index
    pub slot_bits: u8,
    /// Number of bits for generation counter
    pub gen_bits: u8,
    /// Number of bits for data length
    pub len_bits: u8,
}

/// A handle for accessing allocated memory with generation tracking
///
/// Handles combine a slot index with a generation counter to prevent use-after-free.
/// When memory is freed, the generation is incremented, invalidating old handles.
///
/// # Examples
///
/// ```
/// use tinyalloc::Handle;
///
/// let handle = Handle::new(5, 2);
/// let (slot, gen) = handle.parts();
/// assert_eq!(slot, 5);
/// assert_eq!(gen, 2);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    /// Slot index in the allocator
    pub slot: u16,
    /// Generation counter for validity checking
    pub generation: u8,
}

impl Handle {
    /// Creates a new handle from a slot index and generation
    pub const fn new(slot: u16, generation: u8) -> Self {
        Self { slot, generation }
    }
    /// Returns the slot index and generation as a tuple
    pub const fn parts(&self) -> (u16, u8) {
        (self.slot, self.generation)
    }
}

/// Core allocator trait for memory management
///
/// Provides a safe interface for allocating, accessing, and freeing memory blocks.
/// All implementations must support generation-tracked handles for safety.
///
/// # Safety Model
///
/// - Handles are validated on every access
/// - Freed memory increments generation, invalidating old handles
/// - No undefined behavior from stale handles
///
/// # Examples
///
/// ```
/// use tinyalloc::prelude::*;
///
/// let mut alloc = TinySlabAllocator::<256, 8>::new();
///
/// // Allocate memory
/// let (handle, buf) = alloc.alloc_uninit(32).unwrap();
/// buf[0] = 42;
///
/// // Access memory
/// assert_eq!(alloc.get(handle).unwrap()[0], 42);
///
/// // Free memory
/// assert!(alloc.free(handle));
///
/// // Handle is now invalid
/// assert!(alloc.get(handle).is_none());
/// ```
pub trait Allocator {
    /// Allocates uninitialized memory of the specified length
    ///
    /// Returns a handle and a mutable slice to write data into.
    /// The memory contents are undefined until written.
    ///
    /// # Returns
    ///
    /// - `Some((handle, buffer))` if allocation succeeds
    /// - `None` if out of memory or length exceeds block size
    fn alloc_uninit(&mut self, len: usize) -> Option<(Handle, &mut [u8])>;
    /// Gets a shared reference to allocated memory
    ///
    /// # Returns
    ///
    /// - `Some(slice)` if the handle is valid
    /// - `None` if the handle is invalid (wrong generation or freed)
    fn get(&self, handle: Handle) -> Option<&[u8]>;
    /// Gets a mutable reference to allocated memory
    ///
    /// # Returns
    ///
    /// - `Some(slice)` if the handle is valid
    /// - `None` if the handle is invalid (wrong generation or freed)
    fn get_mut(&mut self, handle: Handle) -> Option<&mut [u8]>;
    /// Frees allocated memory, invalidating the handle
    ///
    /// The generation counter is incremented, making the handle invalid.
    ///
    /// # Returns
    ///
    /// - `true` if the handle was valid and memory was freed
    /// - `false` if the handle was already invalid
    fn free(&mut self, handle: Handle) -> bool;
    /// Returns the number of currently allocated blocks
    fn len(&self) -> usize;

    /// Returns true if no blocks are currently allocated
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Returns the total number of available slots
    fn capacity(&self) -> usize;

    /// Returns the size of each memory block in bytes
    fn block_size(&self) -> usize;
    /// Returns the bit layout used for metadata packing
    fn bit_layout(&self) -> BitLayout;
    /// Allocates memory and copies data into it
    ///
    /// This is a convenience method that combines `alloc_uninit` with `copy_from_slice`.
    ///
    /// # Returns
    ///
    /// - `Some(handle)` if allocation and copy succeed
    /// - `None` if out of memory or data length exceeds block size
    fn alloc(&mut self, data: &[u8]) -> Option<Handle> {
        let (h, buf) = self.alloc_uninit(data.len())?;
        buf.copy_from_slice(data);
        Some(h)
    }
    /// Frees all allocated memory, resetting the allocator
    fn clear(&mut self);
}
