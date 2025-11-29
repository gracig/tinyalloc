use crate::{Allocator, Handle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteBufferError {
    Uninitialized,
    AllocationFailed,
    Full,
}

const NONE_SLOT: u8 = u8::MAX;
const NONE_GEN: u8 = 0;

/// Block layout in arena memory (zero-copy interpretation):
/// [0]       = len (u8) - number of data bytes used
/// [1..n-2]  = data bytes (variable size, depends on allocator)
/// [n-1]     = next_slot (u8) - slot index of next block
/// [n]       = next_gen (u8) - generation of next block

/// Helper functions to access block metadata without Node struct
#[inline]
fn get_len(block: &[u8]) -> u8 {
    block[0]
}

#[inline]
fn set_len(block: &mut [u8], len: u8) {
    block[0] = len;
}

#[inline]
fn get_next_slot(block: &[u8]) -> u8 {
    block[block.len() - 2]
}

#[inline]
fn set_next_slot(block: &mut [u8], slot: u8) {
    let idx = block.len() - 2;
    block[idx] = slot;
}

#[inline]
fn get_next_gen(block: &[u8]) -> u8 {
    block[block.len() - 1]
}

#[inline]
fn set_next_gen(block: &mut [u8], generation: u8) {
    let idx = block.len() - 1;
    block[idx] = generation;
}

#[inline]
fn get_data(block: &[u8]) -> &[u8] {
    let len = get_len(block) as usize;
    &block[1..1 + len]
}

#[inline]
fn get_data_mut(block: &mut [u8]) -> &mut [u8] {
    let len = block.len();
    &mut block[1..len - 2]
}

#[inline]
fn data_capacity(block: &[u8]) -> usize {
    block.len().saturating_sub(3)
}

pub struct ByteBuffer {
    head: Option<Handle>,
    tail: Option<Handle>,
    len: u16,
    max_len: Option<u16>,
}

impl Default for ByteBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ByteBuffer {
    pub const fn new() -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            max_len: None,
        }
    }

    pub const fn with_max_len(max: u16) -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            max_len: Some(max),
        }
    }

    pub fn set_max_len(&mut self, v: Option<u16>) {
        self.max_len = v;
    }
    pub fn max_len(&self) -> Option<u16> {
        self.max_len
    }
    pub fn len(&self) -> u16 {
        self.len
    }
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
        if block_size < 3 {
            return Err(ByteBufferError::AllocationFailed);
        }

        let (h, buf) = arena
            .alloc_uninit(block_size)
            .ok_or(ByteBufferError::AllocationFailed)?;

        set_len(buf, 0);
        set_next_slot(buf, NONE_SLOT);
        set_next_gen(buf, NONE_GEN);
        // Data area is already uninitialized, no need to zero it

        Ok(h)
    }

    fn get_next_handle(block: &[u8]) -> Option<Handle> {
        let slot = get_next_slot(block);
        if slot == NONE_SLOT {
            None
        } else {
            Some(Handle::new(slot as u16, get_next_gen(block)))
        }
    }
}
pub struct ByteBufferRead<'a, A: Allocator> {
    pub(crate) arena: &'a A,
    pub(crate) buf: &'a ByteBuffer,
}

impl<'a, A: Allocator> ByteBufferRead<'a, A> {
    pub fn len(&self) -> u16 {
        self.buf.len
    }

    pub fn chunks(&self) -> ByteChunkIter<'a, A> {
        ByteChunkIter {
            arena: self.arena,
            next_handle: self.buf.head,
            _marker: core::marker::PhantomData,
        }
    }

    pub fn bytes(&self) -> impl Iterator<Item = u8> + 'a {
        self.chunks().flat_map(|c| c.iter().copied())
    }
}
pub struct ByteChunkIter<'a, A: Allocator> {
    pub arena: &'a A,
    pub next_handle: Option<Handle>,
    pub _marker: core::marker::PhantomData<A>,
}

impl<'a, A: Allocator> Iterator for ByteChunkIter<'a, A> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let h = self.next_handle?;
        let block = self.arena.get(h)?;
        let data = get_data(block);
        self.next_handle = ByteBuffer::get_next_handle(block);
        Some(data)
    }
}

pub struct ByteBufferWrite<'a, A: Allocator> {
    pub(crate) arena: &'a mut A,
    pub(crate) buf: &'a mut ByteBuffer,
}

impl<'a, A: Allocator> ByteBufferWrite<'a, A> {
    pub fn clear(&mut self) {
        let mut cursor = self.buf.head;
        while let Some(h) = cursor {
            cursor = {
                let block = self.arena.get(h).unwrap();
                ByteBuffer::get_next_handle(block)
            };
            let _ = self.arena.free(h);
        }
        self.buf.head = None;
        self.buf.tail = None;
        self.buf.len = 0;
    }

    pub fn append(&mut self, b: u8) -> Result<(), ByteBufferError> {
        if let Some(max) = self.buf.max_len {
            if self.buf.len >= max {
                return Err(ByteBufferError::Full);
            }
        }

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
            let len = get_len(block) as usize;
            let capacity = data_capacity(block);
            len >= capacity
        };

        if needs_new_node {
            let block_size = self.arena.block_size();
            let new = self.buf.alloc_node(self.arena, block_size)?;

            // Now update the old tail to point to new node
            let block = self.arena.get_mut(tail).unwrap();
            set_next_slot(block, new.slot as u8);
            set_next_gen(block, new.generation);

            self.buf.tail = Some(new);

            let block = self.arena.get_mut(new).unwrap();
            let data = get_data_mut(block);
            data[0] = b;
            set_len(block, 1);
        } else {
            let block = self.arena.get_mut(tail).unwrap();
            let len = get_len(block) as usize;
            let data = get_data_mut(block);
            data[len] = b;
            set_len(block, (len + 1) as u8);
        }

        self.buf.len += 1;
        Ok(())
    }

    pub fn extend(&mut self, s: &[u8]) -> Result<(), ByteBufferError> {
        for &b in s {
            self.append(b)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::naive::NaiveAllocator;

    #[test]
    fn test_block_layout() {
        // Verify block layout with actual allocator
        let mut alloc = NaiveAllocator::new();
        let (h, block) = alloc.alloc_uninit(11).unwrap();

        set_len(block, 5);
        set_next_slot(block, 42);
        set_next_gen(block, 3);

        let block = alloc.get(h).unwrap();
        assert_eq!(get_len(block), 5);
        assert_eq!(get_next_slot(block), 42);
        assert_eq!(get_next_gen(block), 3);
        assert_eq!(data_capacity(block), 8); // 11 - 3 = 8 bytes for data
    }

    // Helper to collect bytes into a fixed array for testing
    fn collect_bytes<A: Allocator>(reader: &ByteBufferRead<A>, max: usize) -> ([u8; 256], usize) {
        let mut result = [0u8; 256];
        let mut count = 0;
        for (i, b) in reader.bytes().enumerate() {
            if i >= max {
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
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        buf.write(&mut alloc).append(42).unwrap();

        assert_eq!(buf.len(), 1);
        let (bytes, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 1);
        assert_eq!(bytes[0], 42);
    }

    #[test]
    fn test_bytebuffer_append_multiple_bytes() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        let data = b"Hello";
        buf.write(&mut alloc).extend(data).unwrap();

        assert_eq!(buf.len(), 5);
        let (bytes, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 5);
        assert_eq!(&bytes[..5], data);
    }

    #[test]
    fn test_bytebuffer_append_exceeds_chunk() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        // CHUNK is 8, so this should create multiple nodes
        let data = b"Hello World!"; // 12 bytes
        buf.write(&mut alloc).extend(data).unwrap();

        assert_eq!(buf.len(), 12);
        let (bytes, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 12);
        assert_eq!(&bytes[..12], data);
    }

    #[test]
    fn test_bytebuffer_max_len_enforcement() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::with_max_len(5);

        let result = buf.write(&mut alloc).extend(b"Hello");
        assert!(result.is_ok());
        assert_eq!(buf.len(), 5);

        // Try to append one more
        let result = buf.write(&mut alloc).append(b'!');
        assert_eq!(result, Err(ByteBufferError::Full));
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn test_bytebuffer_clear() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        buf.write(&mut alloc).extend(b"Hello World!").unwrap();
        assert_eq!(buf.len(), 12);

        buf.write(&mut alloc).clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());

        let (_, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_bytebuffer_chunks() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        // Write 20 bytes
        let mut data = [0u8; 20];
        for i in 0..20 {
            data[i] = i as u8;
        }
        buf.write(&mut alloc).extend(&data).unwrap();

        let mut chunk_count = 0;
        for _chunk in buf.read(&alloc).chunks() {
            chunk_count += 1;
        }
        // With variable block sizes, we just verify we have at least 1 chunk
        assert!(chunk_count >= 1);

        // Verify all data is readable
        let (reconstructed, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 20);
        assert_eq!(&reconstructed[..20], &data[..]);
    }

    #[test]
    fn test_bytebuffer_move_from() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"Source data").unwrap();
        assert_eq!(src.len(), 11);

        dst.move_from(&mut src);

        // dst should have the data
        assert_eq!(dst.len(), 11);
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 11);
        assert_eq!(&bytes[..11], b"Source data");

        // src should be empty
        assert_eq!(src.len(), 0);
        assert!(src.is_empty());
    }

    #[test]
    fn test_bytebuffer_drain() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        buf.write(&mut alloc).extend(b"Drain this!").unwrap();
        assert_eq!(buf.len(), 11);

        // Drain to new buffer
        let drained = buf.drain();

        // Original should be empty
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());

        // Drained should have the data
        assert_eq!(drained.len(), 11);
        let (bytes, count) = collect_bytes(&drained.read(&alloc), 256);
        assert_eq!(count, 11);
        assert_eq!(&bytes[..11], b"Drain this!");
    }

    #[test]
    fn test_bytebuffer_drain_and_copy_to_another() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        // Add data to source
        src.write(&mut alloc).extend(b"Source").unwrap();
        assert_eq!(src.len(), 6);

        // Add prefix to destination
        dst.write(&mut alloc).extend(b"Prefix:").unwrap();
        assert_eq!(dst.len(), 7);

        // Copy from source to destination (appends to dst)
        dst.copy_from(&mut alloc, &src, 0..src.len() as usize)
            .unwrap();

        // Now dst has both prefix and source data
        assert_eq!(dst.len(), 13); // 7 + 6
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 13);
        assert_eq!(&bytes[..13], b"Prefix:Source");

        // Source still has its data (copy_from doesn't drain)
        assert_eq!(src.len(), 6);
    }

    #[test]
    fn test_bytebuffer_drain_move_and_append() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        // Add prefix to destination
        dst.write(&mut alloc).extend(b"Start:").unwrap();
        assert_eq!(dst.len(), 6);

        // Add data to source
        src.write(&mut alloc).extend(b"Moved").unwrap();
        assert_eq!(src.len(), 5);

        // Copy source data to dst, then drain source
        dst.copy_from(&mut alloc, &src, 0..src.len() as usize)
            .unwrap();
        src.write(&mut alloc).clear(); // Drain/clear source

        assert_eq!(dst.len(), 11);
        assert!(src.is_empty());

        let (bytes, _count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(&bytes[..11], b"Start:Moved");
    }

    #[test]
    fn test_bytebuffer_multiple_copy_append() {
        let mut alloc = NaiveAllocator::new();
        let mut buf1 = ByteBuffer::new();
        let mut buf2 = ByteBuffer::new();
        let mut result = ByteBuffer::new();

        // Fill buffers
        buf1.write(&mut alloc).extend(b"AAA").unwrap();
        buf2.write(&mut alloc).extend(b"BBB").unwrap();

        // Copy both into result (entire buffers)
        result
            .copy_from(&mut alloc, &buf1, 0..buf1.len() as usize)
            .unwrap();
        result
            .copy_from(&mut alloc, &buf2, 0..buf2.len() as usize)
            .unwrap();

        assert_eq!(result.len(), 6);
        let (bytes, _count) = collect_bytes(&result.read(&alloc), 256);
        assert_eq!(&bytes[..6], b"AAABBB");
    }

    #[test]
    fn test_bytebuffer_copy_from_entire() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"Copy this!").unwrap();
        assert_eq!(src.len(), 10);

        // Copy entire buffer using range 0..src.len()
        dst.copy_from(&mut alloc, &src, 0..src.len() as usize)
            .unwrap();

        // Both should have the data
        assert_eq!(dst.len(), 10);
        assert_eq!(src.len(), 10);

        let (src_bytes, src_count) = collect_bytes(&src.read(&alloc), 256);
        let (dst_bytes, dst_count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(src_count, 10);
        assert_eq!(dst_count, 10);
        assert_eq!(&src_bytes[..10], b"Copy this!");
        assert_eq!(&dst_bytes[..10], b"Copy this!");
    }

    #[test]
    fn test_bytebuffer_read_write_lifecycle() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        // Write phase
        {
            let mut writer = buf.write(&mut alloc);
            writer.extend(b"Phase 1").unwrap();
            writer.extend(b" Phase 2").unwrap();
        }

        // Read phase
        {
            let reader = buf.read(&alloc);
            let (bytes, count) = collect_bytes(&reader, 256);
            assert_eq!(count, 15);
            assert_eq!(&bytes[..15], b"Phase 1 Phase 2");
        }

        // Write more
        {
            let mut writer = buf.write(&mut alloc);
            writer.extend(b" Phase 3").unwrap();
        }

        // Read all
        let (bytes, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 23);
        assert_eq!(&bytes[..23], b"Phase 1 Phase 2 Phase 3");
    }

    #[test]
    fn test_bytebuffer_empty_operations() {
        let alloc = NaiveAllocator::new();
        let buf = ByteBuffer::new();

        // Read from empty buffer
        let (_, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 0);

        let mut chunk_count = 0;
        for _ in buf.read(&alloc).chunks() {
            chunk_count += 1;
        }
        assert_eq!(chunk_count, 0);
    }

    #[test]
    fn test_bytebuffer_set_max_len() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        assert_eq!(buf.max_len(), None);

        buf.set_max_len(Some(10));
        assert_eq!(buf.max_len(), Some(10));

        buf.write(&mut alloc).extend(b"12345").unwrap();
        assert_eq!(buf.len(), 5);

        // Can add 5 more to reach limit
        buf.write(&mut alloc).extend(b"67890").unwrap();
        assert_eq!(buf.len(), 10);

        // Try to append one more - should fail
        let result = buf.write(&mut alloc).append(b'X');
        assert_eq!(result, Err(ByteBufferError::Full));
        assert_eq!(buf.len(), 10);

        // Remove limit
        buf.set_max_len(None);
        buf.write(&mut alloc).extend(b"ABCDEFGH").unwrap();
        assert_eq!(buf.len(), 18); // 10 + 8
    }

    #[test]
    fn test_bytebuffer_large_data() {
        let mut alloc = NaiveAllocator::new();
        let mut buf = ByteBuffer::new();

        // Write 100 bytes
        let mut data = [0u8; 100];
        for i in 0..100 {
            data[i] = (i % 256) as u8;
        }
        buf.write(&mut alloc).extend(&data).unwrap();

        assert_eq!(buf.len(), 100);

        let (bytes, count) = collect_bytes(&buf.read(&alloc), 256);
        assert_eq!(count, 100);
        assert_eq!(&bytes[..100], &data[..]);
    }

    #[test]
    fn test_bytebuffer_copy_from_basic() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"0123456789").unwrap();

        // Copy middle range
        dst.copy_from(&mut alloc, &src, 3..7).unwrap();

        assert_eq!(dst.len(), 4);
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 4);
        assert_eq!(&bytes[..4], b"3456");

        // Source should be unchanged
        assert_eq!(src.len(), 10);
    }

    #[test]
    fn test_bytebuffer_copy_from_multiple_times() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"ABCDEFGHIJ").unwrap();

        // Copy multiple ranges to build up destination
        dst.copy_from(&mut alloc, &src, 0..3).unwrap();
        dst.copy_from(&mut alloc, &src, 7..10).unwrap();

        assert_eq!(dst.len(), 6);
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 6);
        assert_eq!(&bytes[..6], b"ABCHIJ");
    }

    #[test]
    fn test_bytebuffer_copy_from_across_chunks() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        // Create data that spans multiple chunks (CHUNK=8)
        let data = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"; // 36 bytes
        src.write(&mut alloc).extend(data).unwrap();

        // Copy range that spans multiple chunks
        dst.copy_from(&mut alloc, &src, 5..25).unwrap();

        assert_eq!(dst.len(), 20);
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 20);
        assert_eq!(&bytes[..20], b"56789ABCDEFGHIJKLMNO");
    }

    #[test]
    fn test_bytebuffer_copy_from_empty_range() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"Hello").unwrap();

        // Copy empty range (start == end)
        dst.copy_from(&mut alloc, &src, 2..2).unwrap();

        assert_eq!(dst.len(), 0);
        assert!(dst.is_empty());
    }

    #[test]
    fn test_bytebuffer_copy_from_to_nonempty_dst() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"ABCDEFGH").unwrap();
        dst.write(&mut alloc).extend(b"123").unwrap();

        // Copy range to existing buffer
        dst.copy_from(&mut alloc, &src, 2..6).unwrap();

        assert_eq!(dst.len(), 7); // 3 + 4
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 7);
        assert_eq!(&bytes[..7], b"123CDEF");
    }

    #[test]
    fn test_bytebuffer_copy_from_single_byte() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"Hello").unwrap();

        // Copy single byte
        dst.copy_from(&mut alloc, &src, 1..2).unwrap();

        assert_eq!(dst.len(), 1);
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 1);
        assert_eq!(&bytes[..1], b"e");
    }

    #[test]
    fn test_bytebuffer_copy_from_full_buffer() {
        let mut alloc = NaiveAllocator::new();
        let mut src = ByteBuffer::new();
        let mut dst = ByteBuffer::new();

        src.write(&mut alloc).extend(b"Complete").unwrap();

        // Copy entire buffer using range
        dst.copy_from(&mut alloc, &src, 0..8).unwrap();

        assert_eq!(dst.len(), 8);
        let (bytes, count) = collect_bytes(&dst.read(&alloc), 256);
        assert_eq!(count, 8);
        assert_eq!(&bytes[..8], b"Complete");
    }
}
