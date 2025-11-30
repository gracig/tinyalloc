//! Dynamic byte buffer backed by allocators
//!
//! `ByteBuffer` provides a growable buffer without heap allocation, using an allocator
//! backend to manage memory as a linked list of chunks.
//!
//! # Features
//!
//! - **Zero-copy operations** - Direct memory access where possible
//! - **Automatic growth** - Allocates new chunks as needed
//! - **Chunk management** - Linked list of fixed-size blocks
//! - **Optional limits** - Enforce maximum buffer size
//! - **Global allocator support** - Works with static global allocators
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use tinyalloc::prelude::*;
//!
//! let mut alloc = TinySlabAllocator::<1024, 32>::new();
//! let mut buf = ByteBuffer::new();
//!
//! // Write data
//! buf.write(&mut alloc, b"Hello, ").unwrap();
//! buf.write(&mut alloc, b"World!").unwrap();
//!
//! // Read back
//! let data = buf.to_vec(&alloc);
//! assert_eq!(&data, b"Hello, World!");
//! ```
//!
//! ## With Global Allocator
//!
//! ```rust,no_run
//! use tinyalloc::global::AllocatorConfig;
//! use tinyalloc::prelude::*;
//!
//! # fn main() {
//! AllocatorConfig::Slab1K32.init();
//!
//! let mut buf = ByteBuffer::new();
//! buf.extend(b"Using global allocator").unwrap();
//! # }
//! ```
//!
//! ## Memory Management
//!
//! ```
//! use tinyalloc::prelude::*;
//!
//! let mut alloc = TinySlabAllocator::<512, 16>::new();
//! let mut buf = ByteBuffer::new();
//!
//! buf.write(&mut alloc, b"data").unwrap();
//! assert_eq!(alloc.len(), 1); // 1 chunk allocated
//!
//! buf.clear(); // Frees all chunks
//! assert_eq!(alloc.len(), 0);
//! ```

use crate::{Allocator, Handle};

/// Errors that can occur during ByteBuffer operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteBufferError {
    /// Global allocator not initialized (call init first)
    Uninitialized,
    /// Allocator out of memory or allocation failed
    AllocationFailed,
    /// Buffer reached maximum length limit
    Full,
}

const NONE_GEN: u8 = 0;

/// Block layout in arena memory (zero-copy interpretation):
/// [0..1]    = metadata (u16) - packed: [gen_bits | len_bits | slot_bits]
/// [2..n]    = data bytes (variable size, depends on allocator)
///
/// The bit layout is determined by allocator.bit_layout() and optimizes
/// space usage based on the allocator's constraints (max slots, generations, block size)
/// Helper functions to access block metadata using bit-packed u16
#[inline(always)]
pub(super) fn get_len(block: &[u8], layout: &crate::BitLayout) -> u8 {
    let metadata = u16::from_le_bytes([block[0], block[1]]);
    let len_mask = (1u16 << layout.len_bits) - 1;
    let len = (metadata >> layout.slot_bits) & len_mask;
    len as u8
}

#[inline(always)]
fn set_len(block: &mut [u8], len: u8, layout: &crate::BitLayout) {
    let mut metadata = u16::from_le_bytes([block[0], block[1]]);
    let len_mask = (1u16 << layout.len_bits) - 1;
    // Clear len bits and set new value
    metadata &= !(len_mask << layout.slot_bits);
    metadata |= ((len as u16) & len_mask) << layout.slot_bits;
    let bytes = metadata.to_le_bytes();
    block[0] = bytes[0];
    block[1] = bytes[1];
}

#[inline(always)]
fn get_next_slot(block: &[u8], layout: &crate::BitLayout) -> u8 {
    let metadata = u16::from_le_bytes([block[0], block[1]]);
    let slot_mask = (1u16 << layout.slot_bits) - 1;
    let slot = metadata & slot_mask;
    slot as u8
}

#[inline(always)]
fn set_next_slot(block: &mut [u8], slot: u8, layout: &crate::BitLayout) {
    let mut metadata = u16::from_le_bytes([block[0], block[1]]);
    let slot_mask = (1u16 << layout.slot_bits) - 1;
    // Clear slot bits and set new value
    metadata &= !slot_mask;
    metadata |= (slot as u16) & slot_mask;
    let bytes = metadata.to_le_bytes();
    block[0] = bytes[0];
    block[1] = bytes[1];
}

#[inline]
fn get_next_gen(block: &[u8], layout: &crate::BitLayout) -> u8 {
    let metadata = u16::from_le_bytes([block[0], block[1]]);
    let gen_shift = layout.slot_bits + layout.len_bits;
    let gen_mask = (1u16 << layout.gen_bits) - 1;
    let generation = (metadata >> gen_shift) & gen_mask;
    generation as u8
}

#[inline]
fn set_next_gen(block: &mut [u8], generation: u8, layout: &crate::BitLayout) {
    let mut metadata = u16::from_le_bytes([block[0], block[1]]);
    let gen_shift = layout.slot_bits + layout.len_bits;
    let gen_mask = (1u16 << layout.gen_bits) - 1;
    // Clear gen bits and set new value
    metadata &= !(gen_mask << gen_shift);
    metadata |= ((generation as u16) & gen_mask) << gen_shift;
    let bytes = metadata.to_le_bytes();
    block[0] = bytes[0];
    block[1] = bytes[1];
}

#[inline]
fn get_data<'a>(block: &'a [u8], layout: &crate::BitLayout) -> &'a [u8] {
    let len = get_len(block, layout) as usize;
    &block[2..2 + len]
}

#[inline]
fn get_data_mut(block: &mut [u8]) -> &mut [u8] {
    &mut block[2..]
}

#[inline]
fn data_capacity(block: &[u8]) -> usize {
    block.len().saturating_sub(2)
}

#[inline(always)]
pub(super) fn get_next_handle(block: &[u8], layout: &crate::BitLayout) -> Option<Handle> {
    let slot = get_next_slot(block, layout);
    let generation = get_next_gen(block, layout);
    let max_slot = ((1u16 << layout.slot_bits) - 1) as u8;
    if slot == max_slot {
        None
    } else {
        Some(Handle::new(slot as u16, generation))
    }
}

/// A dynamic byte buffer backed by allocator chunks
///
/// Stores data as a linked list of fixed-size blocks allocated from an `Allocator`.
/// Automatically grows by allocating new chunks as needed.
///
/// # Examples
///
/// ```
/// use tinyalloc::prelude::*;
///
/// let mut alloc = TinySlabAllocator::<512, 16>::new();
/// let mut buf = ByteBuffer::new();
///
/// buf.write(&mut alloc, b"Hello").unwrap();
/// assert_eq!(buf.len(), 5);
/// ```
pub struct ByteBuffer {
    /// Head of the chunk linked list
    pub(super) head: Option<Handle>,
    /// Tail of the chunk linked list
    pub(super) tail: Option<Handle>,
    /// Total number of bytes stored
    pub(super) len: u16,
    /// Optional maximum length limit
    pub(super) max_len: Option<u16>,
}

impl Default for ByteBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ByteBuffer {
    /// Creates a new empty ByteBuffer
    ///
    /// # Examples
    ///
    /// ```
    /// use tinyalloc::prelude::*;
    /// let buf = ByteBuffer::new();
    /// assert_eq!(buf.len(), 0);
    /// ```
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            max_len: None,
        }
    }

    /// Creates a new ByteBuffer with a maximum length limit
    ///
    /// # Examples
    ///
    /// ```
    /// use tinyalloc::prelude::*;
    /// let buf = ByteBuffer::with_max_len(100);
    /// assert_eq!(buf.max_len(), Some(100));
    /// ```
    #[inline(always)]
    pub const fn with_max_len(max: u16) -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            max_len: Some(max),
        }
    }

    /// Sets the maximum length limit
    ///
    /// Pass `None` to remove the limit.
    #[inline(always)]
    pub fn set_max_len(&mut self, v: Option<u16>) {
        self.max_len = v;
    }
    /// Returns the current maximum length limit
    #[inline(always)]
    pub fn max_len(&self) -> Option<u16> {
        self.max_len
    }
    /// Returns the number of bytes in the buffer
    #[inline(always)]
    pub fn len(&self) -> u16 {
        self.len
    }
    /// Returns true if the buffer is empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Immutable read context
    pub fn read<'a, A: Allocator>(&'a self, arena: &'a A) -> ByteBufferRead<'a, A> {
        ByteBufferRead { arena, buf: self }
    }

    /// Mutable write context
    pub fn write<'a, A: Allocator>(&'a mut self, arena: &'a mut A) -> ByteBufferWrite<'a, A> {
        ByteBufferWrite { arena, buf: self }
    }

    /// Zero-copy transfer
    pub fn move_from(&mut self, src: &mut Self) {
        core::mem::swap(self, src);
        src.head = None;
        src.tail = None;
        src.len = 0;
    }

    /// Zero-copy drain to a new buffer - empties self and returns new buffer with the data
    pub fn drain(&mut self) -> Self {
        let mut drained = Self::new();
        drained.move_from(self);
        drained
    }

    /// Copies bytes from a specific range of another ByteBuffer into this one.
    /// This allows reading from one buffer and appending to another using the same allocator.
    /// Uses unsafe raw pointers internally to avoid borrow checker conflicts.
    pub fn copy_from<A: Allocator>(
        &mut self,
        arena: &mut A,
        src: &Self,
        range: core::ops::Range<usize>,
    ) -> Result<(), ByteBufferError> {
        // Use raw pointer to arena to allow simultaneous read and write
        let arena_ptr = arena as *mut A;

        unsafe {
            for byte in src
                .read(&*arena)
                .bytes()
                .skip(range.start)
                .take(range.end - range.start)
            {
                self.write(&mut *arena_ptr).append(byte)?;
            }
        }
        Ok(())
    }

    /* ---- Internal helpers ---- */

    fn alloc_node<A: Allocator>(
        &self,
        arena: &mut A,
        block_size: usize,
    ) -> Result<Handle, ByteBufferError> {
        if block_size < 2 {
            return Err(ByteBufferError::AllocationFailed);
        }

        let layout = arena.bit_layout();
        let (h, buf) = arena
            .alloc_uninit(block_size)
            .ok_or(ByteBufferError::AllocationFailed)?;

        let none_slot = ((1u16 << layout.slot_bits) - 1) as u8;
        set_len(buf, 0, &layout);
        set_next_slot(buf, none_slot, &layout);
        set_next_gen(buf, NONE_GEN, &layout);
        // Data area is already uninitialized, no need to zero it

        Ok(h)
    }
}

// ============================================================================
// Global Allocator API (enabled with feature = "global-alloc")
// ============================================================================

#[cfg(feature = "global-alloc")]
impl ByteBuffer {
    /// Append a single byte using the global allocator
    pub fn append(&mut self, byte: u8) -> Result<(), ByteBufferError> {
        crate::global::with_global_allocator(|alloc| self.write(alloc).append(byte))
    }

    /// Extend with multiple bytes using the global allocator
    pub fn extend(&mut self, data: &[u8]) -> Result<(), ByteBufferError> {
        crate::global::with_global_allocator(|alloc| self.write(alloc).extend(data))
    }

    /// Clear all data using the global allocator
    pub fn clear(&mut self) {
        crate::global::with_global_allocator(|alloc| self.write(alloc).clear())
    }

    /// Iterate over bytes using the global allocator
    pub fn bytes(&self) -> ByteBufferGlobalIter<'_> {
        ByteBufferGlobalIter {
            buf: self,
            index: 0,
        }
    }

    /// Copy from another buffer using global allocator
    pub fn copy_from_global(
        &mut self,
        src: &Self,
        range: core::ops::Range<usize>,
    ) -> Result<(), ByteBufferError> {
        crate::global::with_global_allocator(|alloc| self.copy_from(alloc, src, range))
    }
}

/// Iterator over bytes in a ByteBuffer using the global allocator
#[cfg(feature = "global-alloc")]
pub struct ByteBufferGlobalIter<'a> {
    buf: &'a ByteBuffer,
    index: usize,
}

#[cfg(feature = "global-alloc")]
impl<'a> Iterator for ByteBufferGlobalIter<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.buf.len as usize {
            return None;
        }

        let byte = crate::global::with_global_allocator(|alloc| {
            self.buf.read(alloc).bytes().nth(self.index)
        });

        self.index += 1;
        byte
    }
}

// ============================================================================

/// Read-only view of a ByteBuffer with an allocator
///
/// Provides methods to read data without modifying the buffer.
pub struct ByteBufferRead<'a, A: Allocator> {
    /// Reference to the allocator
    pub(crate) arena: &'a A,
    /// Reference to the buffer
    pub(crate) buf: &'a ByteBuffer,
}

impl<'a, A: Allocator> ByteBufferRead<'a, A> {
    /// Returns the number of bytes in the buffer
    pub fn len(&self) -> u16 {
        self.buf.len
    }

    /// Returns true if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buf.len == 0
    }

    /// Returns an iterator over the chunks in the buffer
    pub fn chunks(&self) -> ByteChunkIter<'a, A> {
        ByteChunkIter {
            arena: self.arena,
            next_handle: self.buf.head,
            _marker: core::marker::PhantomData,
        }
    }

    /// Returns an iterator over individual bytes in the buffer
    pub fn bytes(&self) -> impl Iterator<Item = u8> + 'a {
        self.chunks().flat_map(|c| c.iter().copied())
    }
}
/// Iterator over chunks in a ByteBuffer
///
/// Each item is a slice representing one allocated chunk.
pub struct ByteChunkIter<'a, A: Allocator> {
    /// Reference to the allocator
    pub arena: &'a A,
    /// Handle to the next chunk
    pub next_handle: Option<Handle>,
    /// Phantom data for lifetime
    pub _marker: core::marker::PhantomData<A>,
}

impl<'a, A: Allocator> Iterator for ByteChunkIter<'a, A> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let h = self.next_handle?;
        let block = self.arena.get(h)?;
        let layout = self.arena.bit_layout();
        let data = get_data(block, &layout);
        self.next_handle = get_next_handle(block, &layout);
        Some(data)
    }
}

/// Mutable view of a ByteBuffer with an allocator
///
/// Provides methods to modify the buffer contents.
pub struct ByteBufferWrite<'a, A: Allocator> {
    /// Mutable reference to the allocator
    pub(crate) arena: &'a mut A,
    /// Mutable reference to the buffer
    pub(crate) buf: &'a mut ByteBuffer,
}

impl<'a, A: Allocator> ByteBufferWrite<'a, A> {
    /// Clears the buffer, freeing all allocated chunks
    pub fn clear(&mut self) {
        let layout = self.arena.bit_layout();
        let mut cursor = self.buf.head;
        while let Some(h) = cursor {
            cursor = {
                if let Some(block) = self.arena.get(h) {
                    get_next_handle(block, &layout)
                } else {
                    None
                }
            };
            let _ = self.arena.free(h);
        }
        self.buf.head = None;
        self.buf.tail = None;
        self.buf.len = 0;
    }

    /// Appends a single byte to the buffer
    ///
    /// # Errors
    ///
    /// Returns `ByteBufferError::Full` if maximum length is reached.
    /// Returns `ByteBufferError::AllocationFailed` if allocator is out of memory.
    pub fn append(&mut self, b: u8) -> Result<(), ByteBufferError> {
        if let Some(max) = self.buf.max_len
            && self.buf.len >= max
        {
            return Err(ByteBufferError::Full);
        }

        let layout = self.arena.bit_layout();

        if self.buf.tail.is_none() {
            // Request allocator's block size
            let block_size = self.arena.block_size();
            let h = self.buf.alloc_node(self.arena, block_size)?;
            self.buf.head = Some(h);
            self.buf.tail = Some(h);
        }

        let tail = self.buf.tail.unwrap();

        // Check if we need a new node first
        let needs_new_node = {
            let block = self
                .arena
                .get_mut(tail)
                .ok_or(ByteBufferError::Uninitialized)?;
            let len = get_len(block, &layout) as usize;
            let capacity = data_capacity(block);
            len >= capacity
        };

        if needs_new_node {
            let block_size = self.arena.block_size();
            let new = self.buf.alloc_node(self.arena, block_size)?;

            // Now update the old tail to point to new node
            let block = self
                .arena
                .get_mut(tail)
                .ok_or(ByteBufferError::Uninitialized)?;
            set_next_slot(block, new.slot as u8, &layout);
            set_next_gen(block, new.generation, &layout);

            self.buf.tail = Some(new);

            let block = self
                .arena
                .get_mut(new)
                .ok_or(ByteBufferError::Uninitialized)?;
            let data = get_data_mut(block);
            data[0] = b;
            set_len(block, 1, &layout);
        } else {
            let block = self
                .arena
                .get_mut(tail)
                .ok_or(ByteBufferError::Uninitialized)?;
            let len = get_len(block, &layout) as usize;
            let data = get_data_mut(block);
            data[len] = b;
            set_len(block, (len + 1) as u8, &layout);
        }

        self.buf.len += 1;
        Ok(())
    }

    /// Appends a slice of bytes to the buffer
    ///
    /// # Errors
    ///
    /// Returns `ByteBufferError::Full` if maximum length would be exceeded.
    /// Returns `ByteBufferError::AllocationFailed` if allocator is out of memory.
    pub fn extend(&mut self, s: &[u8]) -> Result<(), ByteBufferError> {
        for &b in s {
            self.append(b)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "tinyslab")]
mod tests {
    use super::*;
    use crate::backend::tinyslab::TinySlabAllocator;

    // Helper to collect bytes from iterator into fixed array
    fn collect_bytes<A: Allocator>(buf: &ByteBuffer, arena: &A) -> ([u8; 256], usize) {
        let mut result = [0u8; 256];
        let mut count = 0;
        for (i, b) in buf.read(arena).bytes().enumerate() {
            if i >= 256 {
                break;
            }
            result[i] = b;
            count += 1;
        }
        (result, count)
    }

    #[test]
    fn test_bytebuffer_new() {
        let buf = ByteBuffer::new();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.max_len(), None);
    }

    #[test]
    fn test_bytebuffer_with_max_len() {
        let buf = ByteBuffer::with_max_len(100);
        assert_eq!(buf.max_len(), Some(100));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_bytebuffer_append_single_byte() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buf = ByteBuffer::new();

        buf.write(&mut arena).append(42).unwrap();

        assert_eq!(buf.len(), 1);
        let (bytes, count) = collect_bytes(&buf, &arena);
        assert_eq!(count, 1);
        assert_eq!(bytes[0], 42);
    }

    #[test]
    fn test_bytebuffer_append_multiple_bytes() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buf = ByteBuffer::new();

        let data = b"Hello";
        buf.write(&mut arena).extend(data).unwrap();

        assert_eq!(buf.len(), 5);
        let (bytes, count) = collect_bytes(&buf, &arena);
        assert_eq!(count, 5);
        assert_eq!(&bytes[..5], data);
    }

    #[test]
    fn test_bytebuffer_append_exceeds_chunk() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buf = ByteBuffer::new();

        let data = b"Hello World!"; // 12 bytes
        buf.write(&mut arena).extend(data).unwrap();

        assert_eq!(buf.len(), 12);
        let (bytes, count) = collect_bytes(&buf, &arena);
        assert_eq!(count, 12);
        assert_eq!(&bytes[..12], data);
    }

    #[test]
    fn test_bytebuffer_max_len_enforcement() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buf = ByteBuffer::with_max_len(5);

        let result = buf.write(&mut arena).extend(b"Hello");
        assert!(result.is_ok());
        assert_eq!(buf.len(), 5);

        // Try to append one more
        let result = buf.write(&mut arena).append(b'!');
        assert_eq!(result, Err(ByteBufferError::Full));
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn test_bytebuffer_clear() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buf = ByteBuffer::new();

        buf.write(&mut arena).extend(b"Hello World!").unwrap();
        assert_eq!(buf.len(), 12);

        buf.write(&mut arena).clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());

        let (_, count) = collect_bytes(&buf, &arena);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_bytebuffer_move_from() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut arena).extend(b"Source data").unwrap();
        assert_eq!(src.len(), 11);

        dst.move_from(&mut src);

        // dst should have the data
        assert_eq!(dst.len(), 11);
        let (bytes, count) = collect_bytes(&dst, &arena);
        assert_eq!(count, 11);
        assert_eq!(&bytes[..11], b"Source data");

        // src should be empty
        assert_eq!(src.len(), 0);
        assert!(src.is_empty());
    }

    #[test]
    fn test_bytebuffer_drain() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buf = ByteBuffer::new();

        buf.write(&mut arena).extend(b"Drain this!").unwrap();
        assert_eq!(buf.len(), 11);

        // Drain to new buffer
        let drained = buf.drain();

        // Original should be empty
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());

        // Drained should have the data
        assert_eq!(drained.len(), 11);
        let (bytes, count) = collect_bytes(&drained, &arena);
        assert_eq!(count, 11);
        assert_eq!(&bytes[..11], b"Drain this!");
    }

    #[test]
    fn test_bytebuffer_copy_from() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut arena).extend(b"0123456789").unwrap();

        // Copy middle range
        dst.copy_from(&mut arena, &src, 3..7).unwrap();

        assert_eq!(dst.len(), 4);
        let (bytes, count) = collect_bytes(&dst, &arena);
        assert_eq!(count, 4);
        assert_eq!(&bytes[..4], b"3456");

        // Source should be unchanged
        assert_eq!(src.len(), 10);
    }

    #[test]
    fn test_bytebuffer_large_data() {
        let mut arena = TinySlabAllocator::<4096, 128>::new();
        let mut buf = ByteBuffer::new();

        // Write 100 bytes
        let mut data = [0u8; 100];
        for i in 0..100 {
            data[i] = (i % 256) as u8;
        }
        buf.write(&mut arena).extend(&data).unwrap();

        assert_eq!(buf.len(), 100);

        let (bytes, count) = collect_bytes(&buf, &arena);
        assert_eq!(count, 100);
        assert_eq!(&bytes[..100], &data[..]);
    }

    #[test]
    fn test_bytebuffer_set_max_len() {
        let mut arena = TinySlabAllocator::<2048, 64>::new();
        let mut buf = ByteBuffer::new();

        assert_eq!(buf.max_len(), None);

        buf.set_max_len(Some(10));
        assert_eq!(buf.max_len(), Some(10));

        buf.write(&mut arena).extend(b"12345").unwrap();
        assert_eq!(buf.len(), 5);

        // Can add 5 more to reach limit
        buf.write(&mut arena).extend(b"67890").unwrap();
        assert_eq!(buf.len(), 10);

        // Try to append one more - should fail
        let result = buf.write(&mut arena).append(b'X');
        assert_eq!(result, Err(ByteBufferError::Full));
        assert_eq!(buf.len(), 10);

        // Remove limit
        buf.set_max_len(None);
        buf.write(&mut arena).extend(b"ABCDEFGH").unwrap();
        assert_eq!(buf.len(), 18);
    }
}
