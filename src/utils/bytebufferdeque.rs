//! Double-ended queue for stream processing
//!
//! `ByteBufferDeque` provides efficient FIFO operations for data streams,
//! backed by `ByteBuffer` with automatic memory reclamation.
//!
//! # Use Cases
//!
//! - **UART/SPI buffers** - Interrupt handler pushes, main loop pops
//! - **Packet processing** - Stream parsing with consume-as-you-go
//! - **Ring buffer replacement** - Dynamic sizing without wrap-around logic
//!
//! # Features
//!
//! - Push to back, pop from front (FIFO)
//! - Automatic chunk freeing when fully consumed
//! - Read tracking without copying data
//! - Optional maximum length enforcement
//!
//! # Examples
//!
//! ## Basic FIFO Operations
//!
//! ```
//! use tinyalloc::prelude::*;
//!
//! let mut alloc = TinySlabAllocator::<512, 16>::new();
//! let mut deque = ByteBufferDeque::new();
//!
//! // Push data
//! deque.push_back(&mut alloc, b'H').unwrap();
//! deque.push_back(&mut alloc, b'i').unwrap();
//!
//! assert_eq!(deque.len(), 2);
//!
//! // Pop data
//! assert_eq!(deque.pop_front(&alloc), Some(b'H'));
//! assert_eq!(deque.pop_front(&alloc), Some(b'i'));
//! assert_eq!(deque.pop_front(&alloc), None);
//! ```
//!
//! ## With Global Allocator (Interrupt Pattern)
//!
//! ```rust,no_run
//! use tinyalloc::prelude::*;
//! use tinyalloc::global::AllocatorConfig;
//!
//! static UART_QUEUE: GlobalDeque = GlobalDeque::new();
//!
//! fn interrupt_handler() {
//!     let byte = read_uart_byte();
//!     UART_QUEUE.push(byte).ok();
//! }
//!
//! fn main() {
//!     AllocatorConfig::Slab512b16.init();
//!     
//!     loop {
//!         while let Some(byte) = UART_QUEUE.pop() {
//!             process_byte(byte);
//!         }
//!     }
//! }
//! # fn read_uart_byte() -> u8 { 0 }
//! # fn process_byte(_: u8) {}
//! ```
//!
//! ## Memory Reclamation
//!
//! ```
//! use tinyalloc::prelude::*;
//!
//! let mut alloc = TinySlabAllocator::<256, 8>::new();
//! let mut deque = ByteBufferDeque::new();
//!
//! // Fill first chunk (assume 30 bytes usable per chunk)
//! for i in 0..30 {
//!     deque.push_back(&mut alloc, i).unwrap();
//! }
//! assert_eq!(alloc.len(), 1); // 1 chunk allocated
//!
//! // Add more, triggering second chunk
//! deque.push_back(&mut alloc, 42).unwrap();
//! assert_eq!(alloc.len(), 2); // 2 chunks allocated
//!
//! // Consume first chunk completely
//! for _ in 0..30 {
//!     deque.pop_front(&alloc);
//! }
//!
//! // First chunk automatically freed
//! assert_eq!(alloc.len(), 1); // Back to 1 chunk
//! ```

use super::bytebuffer::{ByteBuffer, ByteBufferError};
use crate::Allocator;

/// A double-ended queue backed by ByteBuffer
///
/// Provides efficient FIFO operations with automatic memory management.
/// Tracks a read offset to avoid copying data, and frees fully-consumed chunks.
///
/// # Memory Model
///
/// - Push operations append to the end (like a queue)
/// - Pop operations read from the front (FIFO)
/// - Read offset tracks consumption without moving data
/// - Chunks are freed when fully read
///
/// # Performance
///
/// | Operation | Complexity | Notes |
/// |-----------|------------|-------|
/// | push_back | O(1) amortized | May allocate new chunk |
/// | pop_front | O(1) | May free consumed chunk |
/// | peek_front | O(1) | No allocation |
/// | len | O(1) | Cached value |
pub struct ByteBufferDeque {
    buffer: ByteBuffer,
    read_offset: u16,
}

impl Default for ByteBufferDeque {
    fn default() -> Self {
        Self::new()
    }
}

impl ByteBufferDeque {
    /// Creates a new empty deque
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            buffer: ByteBuffer::new(),
            read_offset: 0,
        }
    }

    /// Creates a new deque with a maximum length limit
    #[inline(always)]
    pub const fn with_max_len(max: u16) -> Self {
        Self {
            buffer: ByteBuffer::with_max_len(max),
            read_offset: 0,
        }
    }

    /// Sets the maximum length limit
    #[inline(always)]
    pub fn set_max_len(&mut self, v: Option<u16>) {
        self.buffer.set_max_len(v);
    }

    /// Returns the current maximum length limit
    #[inline(always)]
    pub fn max_len(&self) -> Option<u16> {
        self.buffer.max_len()
    }

    /// Returns the number of bytes currently available to read
    #[inline]
    pub fn len(&self) -> u16 {
        self.buffer.len().saturating_sub(self.read_offset)
    }

    /// Returns true if the deque is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total bytes written (includes consumed bytes)
    pub fn total_written(&self) -> u16 {
        self.buffer.len()
    }

    /// Total bytes consumed from the front
    pub fn total_read(&self) -> u16 {
        self.read_offset
    }

    /// Push a byte to the back of the deque
    pub fn push_back<A: Allocator>(
        &mut self,
        arena: &mut A,
        byte: u8,
    ) -> Result<(), ByteBufferError> {
        self.buffer.write(arena).append(byte)
    }

    /// Push multiple bytes to the back of the deque
    pub fn extend<A: Allocator>(
        &mut self,
        arena: &mut A,
        data: &[u8],
    ) -> Result<(), ByteBufferError> {
        self.buffer.write(arena).extend(data)
    }

    /// Pop a byte from the front of the deque
    pub fn pop_front<A: Allocator>(&mut self, arena: &mut A) -> Option<u8> {
        if self.is_empty() {
            return None;
        }

        // Read the byte at current read offset
        let byte = self
            .buffer
            .read(arena)
            .bytes()
            .nth(self.read_offset as usize)?;

        self.read_offset += 1;

        // Try to free fully-consumed head chunks
        self.try_free_head_chunks(arena);

        Some(byte)
    }

    /// Peek at the front byte without consuming it
    pub fn peek_front<A: Allocator>(&self, arena: &A) -> Option<u8> {
        if self.is_empty() {
            return None;
        }

        self.buffer
            .read(arena)
            .bytes()
            .nth(self.read_offset as usize)
    }

    /// Clear all data and reset offsets
    pub fn clear<A: Allocator>(&mut self, arena: &mut A) {
        self.buffer.write(arena).clear();
        self.read_offset = 0;
    }

    /// Drain all available bytes into a new ByteBuffer
    pub fn drain<A: Allocator>(&mut self, arena: &mut A) -> ByteBuffer {
        let mut drained = ByteBuffer::new();

        // Copy remaining bytes
        while let Some(byte) = self.pop_front(arena) {
            let _ = drained.write(arena).append(byte);
        }

        drained
    }

    /// Try to free head chunks that have been fully consumed
    fn try_free_head_chunks<A: Allocator>(&mut self, arena: &mut A) {
        let layout = arena.bit_layout();
        let mut _bytes_freed = 0u16;

        // Walk through chunks and free those that are fully consumed
        while let Some(head) = self.buffer.read(arena).buf.head {
            let block = match arena.get(head) {
                Some(b) => b,
                None => break,
            };

            let chunk_len = super::bytebuffer::get_len(block, &layout) as u16;

            if self.read_offset >= chunk_len {
                // This chunk is fully consumed, free it
                let next = super::bytebuffer::get_next_handle(block, &layout);

                // Now free the head node and update buffer internals
                if arena.free(head) {
                    _bytes_freed += chunk_len;
                    self.read_offset -= chunk_len;

                    // Update buffer head (accessing internals via unsafe - we know the layout)
                    // Safety: We own &mut self, so we can mutate buffer
                    unsafe {
                        let buffer_ptr = &mut self.buffer as *mut ByteBuffer;
                        (*buffer_ptr).head = next;
                        if next.is_none() {
                            (*buffer_ptr).tail = None;
                        }
                        (*buffer_ptr).len = (*buffer_ptr).len.saturating_sub(chunk_len);
                    }
                } else {
                    break;
                }
            } else {
                // Head chunk still has unread data
                break;
            }
        }
    }
}

impl From<ByteBuffer> for ByteBufferDeque {
    fn from(buffer: ByteBuffer) -> Self {
        Self {
            buffer,
            read_offset: 0,
        }
    }
}

// ============================================================================
// Global Deque Wrapper (enabled with feature = "global-alloc")
// ============================================================================

#[cfg(feature = "global-alloc")]
use core::cell::UnsafeCell;

/// Global wrapper for ByteBufferDeque (single-threaded, no mutex needed)
///
/// This provides a safe interface for sharing a ByteBufferDeque globally
/// in single-threaded embedded contexts (e.g., between interrupt handlers
/// and main code).
///
/// # Safety
/// This is only safe in single-threaded contexts. Do not use with multiple
/// threads or concurrent access.
#[cfg(feature = "global-alloc")]
pub struct GlobalDeque {
    inner: UnsafeCell<ByteBufferDeque>,
}

#[cfg(feature = "global-alloc")]
unsafe impl Sync for GlobalDeque {}

#[cfg(feature = "global-alloc")]
impl GlobalDeque {
    /// Create a new global deque
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(ByteBufferDeque::new()),
        }
    }
}

#[cfg(feature = "global-alloc")]
impl Default for GlobalDeque {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "global-alloc")]
impl GlobalDeque {
    /// Create a new global deque with maximum length
    pub const fn with_max_len(max: u16) -> Self {
        Self {
            inner: UnsafeCell::new(ByteBufferDeque::with_max_len(max)),
        }
    }

    /// Execute a closure with mutable access to the deque
    ///
    /// # Safety
    /// Safe in single-threaded context. Caller must ensure no concurrent access.
    #[inline]
    pub fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ByteBufferDeque) -> R,
    {
        unsafe { f(&mut *self.inner.get()) }
    }

    /// Push a byte using the global allocator
    #[inline]
    pub fn push(&self, byte: u8) -> Result<(), super::bytebuffer::ByteBufferError> {
        self.with_mut(|deque| {
            crate::global::with_global_allocator(|alloc| deque.push_back(alloc, byte))
        })
    }

    /// Pop a byte using the global allocator
    #[inline]
    pub fn pop(&self) -> Option<u8> {
        self.with_mut(|deque| crate::global::with_global_allocator(|alloc| deque.pop_front(alloc)))
    }

    /// Peek at the front byte using the global allocator
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        self.with_mut(|deque| crate::global::with_global_allocator(|alloc| deque.peek_front(alloc)))
    }

    /// Get the current length
    #[inline]
    pub fn len(&self) -> u16 {
        self.with_mut(|deque| deque.len())
    }

    /// Check if the deque is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.with_mut(|deque| deque.is_empty())
    }

    /// Clear all data using the global allocator
    #[inline]
    pub fn clear(&self) {
        self.with_mut(|deque| crate::global::with_global_allocator(|alloc| deque.clear(alloc)))
    }
}

#[cfg(test)]
#[cfg(feature = "tinyslab")]
mod tests {
    use super::*;
    use crate::backend::tinyslab::TinySlabAllocator;

    #[test]
    fn test_deque_new() {
        let deque = ByteBufferDeque::new();
        assert_eq!(deque.len(), 0);
        assert!(deque.is_empty());
    }

    #[test]
    fn test_deque_push_pop() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut deque = ByteBufferDeque::new();

        deque.push_back(&mut arena, 1).unwrap();
        deque.push_back(&mut arena, 2).unwrap();
        deque.push_back(&mut arena, 3).unwrap();

        assert_eq!(deque.len(), 3);
        assert_eq!(deque.pop_front(&mut arena), Some(1));
        assert_eq!(deque.pop_front(&mut arena), Some(2));
        assert_eq!(deque.len(), 1);
        assert_eq!(deque.pop_front(&mut arena), Some(3));
        assert_eq!(deque.pop_front(&mut arena), None);
        assert!(deque.is_empty());
    }

    #[test]
    fn test_deque_peek() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut deque = ByteBufferDeque::new();

        deque.extend(&mut arena, b"ABC").unwrap();

        assert_eq!(deque.peek_front(&arena), Some(b'A'));
        assert_eq!(deque.len(), 3);

        assert_eq!(deque.pop_front(&mut arena), Some(b'A'));
        assert_eq!(deque.peek_front(&arena), Some(b'B'));
    }

    #[test]
    fn test_deque_mixed_operations() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut deque = ByteBufferDeque::new();

        // Push some data
        deque.extend(&mut arena, b"Hello").unwrap();
        assert_eq!(deque.len(), 5);

        // Pop some
        assert_eq!(deque.pop_front(&mut arena), Some(b'H'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'e'));
        assert_eq!(deque.len(), 3);

        // Push more
        deque.extend(&mut arena, b" World").unwrap();
        assert_eq!(deque.len(), 9); // "llo World"

        // Pop and verify each byte
        assert_eq!(deque.pop_front(&mut arena), Some(b'l'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'l'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'o'));
        assert_eq!(deque.pop_front(&mut arena), Some(b' '));
        assert_eq!(deque.pop_front(&mut arena), Some(b'W'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'o'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'r'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'l'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'd'));
        assert!(deque.is_empty());
    }

    #[test]
    fn test_deque_iter() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut deque = ByteBufferDeque::new();

        deque.extend(&mut arena, b"12345").unwrap();
        deque.pop_front(&mut arena); // Remove '1'
        deque.pop_front(&mut arena); // Remove '2'

        // Verify remaining bytes
        assert_eq!(deque.pop_front(&mut arena), Some(b'3'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'4'));
        assert_eq!(deque.pop_front(&mut arena), Some(b'5'));
        assert!(deque.is_empty());
    }

    #[test]
    fn test_deque_clear() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut deque = ByteBufferDeque::new();

        deque.extend(&mut arena, b"Some data").unwrap();
        assert_eq!(deque.len(), 9);

        deque.clear(&mut arena);
        assert_eq!(deque.len(), 0);
        assert_eq!(deque.read_offset, 0);
        assert!(deque.is_empty());
    }

    #[test]
    fn test_deque_max_len() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut deque = ByteBufferDeque::with_max_len(5);

        deque.extend(&mut arena, b"Hello").unwrap();
        assert_eq!(
            deque.push_back(&mut arena, b'!'),
            Err(ByteBufferError::Full)
        );

        // Pop doesn't immediately free space - max_len tracks total buffer size
        // The underlying buffer still has 5 bytes allocated
        deque.pop_front(&mut arena);
        assert_eq!(deque.len(), 4); // Available to read
        assert_eq!(deque.total_written(), 5); // Still counts toward max_len

        // Still can't add more
        assert_eq!(
            deque.push_back(&mut arena, b'!'),
            Err(ByteBufferError::Full)
        );
    }

    #[test]
    fn test_deque_from_bytebuffer() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buffer = ByteBuffer::new();

        buffer.write(&mut arena).extend(b"Test").unwrap();

        let mut deque = ByteBufferDeque::from(buffer);
        assert_eq!(deque.len(), 4);
        assert_eq!(deque.pop_front(&mut arena), Some(b'T'));
    }

    #[test]
    fn test_deque_drain() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut deque = ByteBufferDeque::new();

        deque.extend(&mut arena, b"Drain me").unwrap();
        deque.pop_front(&mut arena); // Remove 'D'

        let drained = deque.drain(&mut arena);

        assert!(deque.is_empty());
        assert_eq!(drained.len(), 7); // "rain me"

        // Verify drained content byte by byte
        let mut bytes = drained.read(&arena).bytes();
        assert_eq!(bytes.next(), Some(b'r'));
        assert_eq!(bytes.next(), Some(b'a'));
        assert_eq!(bytes.next(), Some(b'i'));
        assert_eq!(bytes.next(), Some(b'n'));
        assert_eq!(bytes.next(), Some(b' '));
        assert_eq!(bytes.next(), Some(b'm'));
        assert_eq!(bytes.next(), Some(b'e'));
        assert_eq!(bytes.next(), None);
    }
}
