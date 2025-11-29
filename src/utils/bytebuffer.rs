use crate::{Allocator, Handle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteBufferError {
    Uninitialized,
    AllocationFailed,
    Full,
}

const NONE_SLOT: u8 = u8::MAX;
const NONE_GEN: u8 = 0;
const CHUNK: usize = 8;

#[repr(C)]
struct Node {
    len: u8,
    data: [u8; CHUNK],
    next_slot: u8,
    next_gen: u8,
}

const fn node_bytes() -> usize {
    core::mem::size_of::<Node>()
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

    fn alloc_node<A: Allocator>(&self, arena: &mut A) -> Result<Handle, ByteBufferError> {
        let (h, buf) = arena
            .alloc_uninit(node_bytes())
            .ok_or(ByteBufferError::AllocationFailed)?;

        let node = Self::node_from_bytes_mut(buf)?;
        node.len = 0;
        node.data.fill(0);
        node.next_slot = NONE_SLOT;
        node.next_gen = NONE_GEN;

        Ok(h)
    }

    fn node_from_bytes(slice: &[u8]) -> Result<&Node, ByteBufferError> {
        if slice.len() != node_bytes() {
            return Err(ByteBufferError::Uninitialized);
        }
        Ok(unsafe { &*(slice.as_ptr() as *const Node) })
    }

    fn node_from_bytes_mut(slice: &mut [u8]) -> Result<&mut Node, ByteBufferError> {
        if slice.len() != node_bytes() {
            return Err(ByteBufferError::Uninitialized);
        }
        Ok(unsafe { &mut *(slice.as_mut_ptr() as *mut Node) })
    }

    fn next_of(node: &Node) -> Option<Handle> {
        if node.next_slot == NONE_SLOT {
            None
        } else {
            Some(Handle::new(node.next_slot as u16, node.next_gen))
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
        let slice = self.arena.get(h)?;
        let node = ByteBuffer::node_from_bytes(slice).ok()?;
        let len = node.len as usize;
        self.next_handle = ByteBuffer::next_of(node);
        Some(&node.data[..len])
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
                let slice = self.arena.get(h).unwrap();
                let node = ByteBuffer::node_from_bytes(slice).unwrap();
                ByteBuffer::next_of(node)
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
            let h = self.buf.alloc_node(self.arena)?;
            self.buf.head = Some(h);
            self.buf.tail = Some(h);
        }

        let tail = self.buf.tail.unwrap();

        // Check if we need a new node first
        let needs_new_node = {
            let slice = self
                .arena
                .get_mut(tail)
                .ok_or(ByteBufferError::Uninitialized)?;
            let node = ByteBuffer::node_from_bytes_mut(slice)?;
            node.len as usize == CHUNK
        };

        if needs_new_node {
            let new = self.buf.alloc_node(self.arena)?;

            // Now update the old tail to point to new node
            let slice = self.arena.get_mut(tail).unwrap();
            let node = ByteBuffer::node_from_bytes_mut(slice)?;
            node.next_slot = new.slot as u8;
            node.next_gen = new.generation;

            self.buf.tail = Some(new);

            let slice = self.arena.get_mut(new).unwrap();
            let n = ByteBuffer::node_from_bytes_mut(slice)?;
            n.data[0] = b;
            n.len = 1;
        } else {
            let slice = self.arena.get_mut(tail).unwrap();
            let node = ByteBuffer::node_from_bytes_mut(slice)?;
            node.data[node.len as usize] = b;
            node.len += 1;
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
    fn test_node_size() {
        // Node is: len(1) + data[8] + next_slot(1) + next_gen(1) = 11 bytes with #[repr(C)]
        assert_eq!(node_bytes(), 11);
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

        // Write 20 bytes - should span multiple CHUNK-sized nodes
        let mut data = [0u8; 20];
        for i in 0..20 {
            data[i] = i as u8;
        }
        buf.write(&mut alloc).extend(&data).unwrap();

        let mut chunk_count = 0;
        for _chunk in buf.read(&alloc).chunks() {
            chunk_count += 1;
        }
        assert!(chunk_count >= 3); // At least 3 chunks for 20 bytes with CHUNK=8

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
