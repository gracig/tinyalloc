use crate::{Allocator, Handle};

const NONE_SLOT: u8 = u8::MAX;

/// Metadata for each slot in the slab allocator
#[derive(Clone, Copy)]
struct SlotMeta {
    generation: u8,
    size: u8,      // actual allocated size in bytes (max 255)
    next_free: u8, // next free slot index (for free list, max 255 slots)
}

/// Fixed-size slab allocator with generation-tracked handles
/// CAPACITY: total memory capacity in bytes
/// SLOTS: number of allocation slots (max 255 for u8 metadata)
/// Block size is CAPACITY / SLOTS
pub struct TinySlabAllocator<const CAPACITY: usize, const SLOTS: usize> {
    memory: [u8; CAPACITY],
    slots: [SlotMeta; SLOTS],
    free_head: u8,  // head of free list (max 255 slots)
    used_count: u8, // number of allocated slots (max 255)
}

impl<const CAPACITY: usize, const SLOTS: usize> TinySlabAllocator<CAPACITY, SLOTS> {
    const BLOCK_SIZE: usize = CAPACITY / SLOTS;

    // Compile-time assertion: SLOTS must fit in u8 (max 255)
    const _ASSERT_SLOTS_FITS_U8: () = assert!(
        SLOTS <= u8::MAX as usize,
        "SLOTS must be <= 255 to fit in u8 metadata"
    );

    pub const fn new() -> Self {
        // Force evaluation of compile-time assertion
        let _ = Self::_ASSERT_SLOTS_FITS_U8;

        Self {
            memory: [0u8; CAPACITY],
            slots: [SlotMeta {
                generation: 0,
                size: 0,
                next_free: NONE_SLOT,
            }; SLOTS],
            free_head: NONE_SLOT,
            used_count: 0,
        }
    }

    fn init_free_list(&mut self) {
        // Build free list linking all slots
        for i in 0..SLOTS {
            self.slots[i].next_free = if i + 1 < SLOTS {
                (i + 1) as u8
            } else {
                NONE_SLOT
            };
        }
        self.free_head = 0;
    }

    #[inline]
    fn slot_offset(&self, slot: u8) -> usize {
        slot as usize * Self::BLOCK_SIZE
    }

    #[inline]
    fn slot_range(&self, slot: u8, size: usize) -> (usize, usize) {
        let start = self.slot_offset(slot);
        (start, start + size)
    }
}

impl<const CAPACITY: usize, const SLOTS: usize> Allocator for TinySlabAllocator<CAPACITY, SLOTS> {
    fn alloc_uninit(&mut self, len: usize) -> Option<(Handle, &mut [u8])> {
        // Check if requested size fits in a block
        if len > Self::BLOCK_SIZE {
            return None;
        }

        // Initialize free list on first allocation
        if self.used_count == 0 && self.free_head == NONE_SLOT {
            self.init_free_list();
        }

        // Check if we have free slots
        if self.free_head == NONE_SLOT {
            return None;
        }

        // Pop from free list
        let slot = self.free_head;
        let meta = &mut self.slots[slot as usize];
        self.free_head = meta.next_free;

        // Update metadata
        meta.size = len as u8;
        meta.next_free = NONE_SLOT;

        let generation = meta.generation;
        self.used_count = self.used_count.saturating_add(1);

        // Return handle and mutable slice
        let (start, end) = self.slot_range(slot, len);
        let handle = Handle::new(slot as u16, generation);
        Some((handle, &mut self.memory[start..end]))
    }

    fn get(&self, handle: Handle) -> Option<&[u8]> {
        let slot_u16 = handle.slot;

        if slot_u16 as usize >= SLOTS {
            return None;
        }

        let slot = slot_u16 as u8;

        let meta = &self.slots[slot as usize];

        // Verify generation and that slot is allocated
        if meta.generation != handle.generation || meta.next_free != NONE_SLOT {
            return None;
        }

        let (start, end) = self.slot_range(slot, meta.size as usize);
        Some(&self.memory[start..end])
    }

    fn get_mut(&mut self, handle: Handle) -> Option<&mut [u8]> {
        let slot_u16 = handle.slot;

        if slot_u16 as usize >= SLOTS {
            return None;
        }

        let slot = slot_u16 as u8;

        let meta = &self.slots[slot as usize];

        // Verify generation and that slot is allocated
        if meta.generation != handle.generation || meta.next_free != NONE_SLOT {
            return None;
        }

        let (start, end) = self.slot_range(slot, meta.size as usize);
        Some(&mut self.memory[start..end])
    }

    fn free(&mut self, handle: Handle) -> bool {
        let slot_u16 = handle.slot;

        if slot_u16 as usize >= SLOTS {
            return false;
        }

        let slot = slot_u16 as u8;

        let meta = &mut self.slots[slot as usize];

        // Verify generation and that slot is allocated
        if meta.generation != handle.generation || meta.next_free != NONE_SLOT {
            return false;
        }

        // Increment generation to invalidate old handles
        meta.generation = meta.generation.wrapping_add(1);
        meta.size = 0;

        // Push to free list
        meta.next_free = self.free_head;
        self.free_head = slot;

        self.used_count = self.used_count.saturating_sub(1);
        true
    }

    fn len(&self) -> usize {
        self.used_count as usize
    }

    fn capacity(&self) -> usize {
        SLOTS
    }

    fn block_size(&self) -> usize {
        Self::BLOCK_SIZE
    }

    fn clear(&mut self) {
        // Reset all metadata
        for slot in &mut self.slots {
            slot.generation = slot.generation.wrapping_add(1);
            slot.size = 0;
            slot.next_free = NONE_SLOT;
        }

        self.used_count = 0;
        self.free_head = NONE_SLOT;

        // Rebuild free list on next allocation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Using CAPACITY and SLOTS: TinySlabAllocator<512, 4> = 512 bytes total, 4 slots = 128 bytes per slot
    type TestAllocator = TinySlabAllocator<512, 4>;

    // This WILL cause a compile error when SLOTS > 255:
    // type TooManySlotsAllocator = TinySlabAllocator<8192, 256>;
    // To verify: let _ = TooManySlotsAllocator::new();

    #[test]
    fn test_slab_new() {
        let alloc = TestAllocator::new();
        assert_eq!(alloc.len(), 0);
        assert_eq!(alloc.capacity(), 4);
    }

    #[test]
    fn test_slab_alloc_and_get() {
        let mut alloc = TestAllocator::new();

        let (handle, buf) = alloc.alloc_uninit(10).unwrap();
        buf.copy_from_slice(b"Hello Test");

        assert_eq!(alloc.len(), 1);

        let data = alloc.get(handle).unwrap();
        assert_eq!(data, b"Hello Test");
    }

    #[test]
    fn test_slab_alloc_too_large() {
        let mut alloc = TestAllocator::new();

        // Try to allocate more than BLOCK_SIZE (128 bytes)
        assert!(alloc.alloc_uninit(200).is_none());
    }

    #[test]
    fn test_slab_free_and_reuse() {
        let mut alloc = TestAllocator::new();

        let (h1, buf1) = alloc.alloc_uninit(5).unwrap();
        buf1.copy_from_slice(b"First");
        assert_eq!(alloc.len(), 1);

        // Free the allocation
        assert!(alloc.free(h1));
        assert_eq!(alloc.len(), 0);

        // Old handle should be invalid
        assert!(alloc.get(h1).is_none());

        // Allocate again - should reuse the slot with new generation
        let (h2, buf2) = alloc.alloc_uninit(6).unwrap();
        buf2.copy_from_slice(b"Second");
        assert_eq!(alloc.len(), 1);

        // New handle should work
        let data = alloc.get(h2).unwrap();
        assert_eq!(data, b"Second");

        // Generations should be different
        assert_ne!(h1.generation, h2.generation);
    }

    #[test]
    fn test_slab_multiple_allocations() {
        let mut alloc = TestAllocator::new();

        let (h1, _) = alloc.alloc_uninit(10).unwrap();
        let (h2, _) = alloc.alloc_uninit(20).unwrap();
        let (h3, _) = alloc.alloc_uninit(30).unwrap();

        assert_eq!(alloc.len(), 3);

        // All handles should be valid
        assert!(alloc.get(h1).is_some());
        assert!(alloc.get(h2).is_some());
        assert!(alloc.get(h3).is_some());
    }

    #[test]
    fn test_slab_capacity_exhaustion() {
        type SmallAlloc = TinySlabAllocator<256, 2>; // 2 slots of 128 bytes each
        let mut alloc = SmallAlloc::new();

        let (h1, _) = alloc.alloc_uninit(10).unwrap();
        let (_h2, _) = alloc.alloc_uninit(20).unwrap();

        assert_eq!(alloc.len(), 2); // Should fail - no more slots
        assert!(alloc.alloc_uninit(30).is_none());

        // Free one slot
        assert!(alloc.free(h1));

        // Now should succeed
        let (h3, _) = alloc.alloc_uninit(40).unwrap();
        assert!(alloc.get(h3).is_some());
    }

    #[test]
    fn test_slab_clear() {
        let mut alloc = TestAllocator::new();

        let (h1, _) = alloc.alloc_uninit(10).unwrap();
        let (h2, _) = alloc.alloc_uninit(20).unwrap();

        assert_eq!(alloc.len(), 2);

        alloc.clear();

        assert_eq!(alloc.len(), 0);

        // Old handles should be invalid
        assert!(alloc.get(h1).is_none());
        assert!(alloc.get(h2).is_none());

        // Should be able to allocate again
        let (h3, _) = alloc.alloc_uninit(30).unwrap();
        assert!(alloc.get(h3).is_some());
    }

    #[test]
    fn test_slab_generation_tracking() {
        let mut alloc = TestAllocator::new();

        let (h1, _) = alloc.alloc_uninit(10).unwrap();
        let old_gen = h1.generation;

        alloc.free(h1);

        let (h2, _) = alloc.alloc_uninit(20).unwrap();

        // Same slot, different generation
        assert_eq!(h1.slot, h2.slot);
        assert_eq!(h2.generation, old_gen.wrapping_add(1));
    }

    #[test]
    fn test_slab_get_mut() {
        let mut alloc = TestAllocator::new();

        let (handle, buf) = alloc.alloc_uninit(10).unwrap();
        buf.copy_from_slice(b"0123456789");

        // Modify through get_mut
        {
            let data = alloc.get_mut(handle).unwrap();
            data[0] = b'X';
            data[9] = b'Y';
        }

        let data = alloc.get(handle).unwrap();
        assert_eq!(data, b"X12345678Y");
    }

    #[test]
    fn test_slab_alloc_helper() {
        let mut alloc = TestAllocator::new();

        let handle = alloc.alloc(b"Hello").unwrap();

        let data = alloc.get(handle).unwrap();
        assert_eq!(data, b"Hello");
    }
}
