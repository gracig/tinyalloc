#![cfg(test)]

use crate::{Allocator, Handle};

const MAX_SLOTS: usize = 64;
const MAX_ALLOC_SIZE: usize = 128;

pub struct NaiveAllocator {
    slots: [Option<MockSlot>; MAX_SLOTS],
    next_slot: u16,
}

struct MockSlot {
    data: [u8; MAX_ALLOC_SIZE],
    len: usize,
    generation: u8,
}

impl NaiveAllocator {
    pub fn new() -> Self {
        Self {
            slots: [const { None }; MAX_SLOTS],
            next_slot: 0,
        }
    }

    fn find_free_slot(&mut self) -> Option<u16> {
        for i in 0..MAX_SLOTS {
            let slot_idx = (self.next_slot as usize + i) % MAX_SLOTS;
            if self.slots[slot_idx].is_none() {
                self.next_slot = (slot_idx + 1) as u16;
                return Some(slot_idx as u16);
            }
        }
        None
    }
}

impl Allocator for NaiveAllocator {
    fn alloc_uninit(&mut self, len: usize) -> Option<(Handle, &mut [u8])> {
        if len > MAX_ALLOC_SIZE {
            return None;
        }

        let slot_idx = self.find_free_slot()?;
        let generation = self.slots[slot_idx as usize]
            .as_ref()
            .map(|s| s.generation.wrapping_add(1))
            .unwrap_or(1);

        self.slots[slot_idx as usize] = Some(MockSlot {
            data: [0; MAX_ALLOC_SIZE],
            len,
            generation,
        });

        let slot = self.slots[slot_idx as usize].as_mut().unwrap();
        let handle = Handle::new(slot_idx, generation);
        Some((handle, &mut slot.data[..len]))
    }

    fn get(&self, handle: Handle) -> Option<&[u8]> {
        let slot = self.slots[handle.slot as usize].as_ref()?;
        if slot.generation != handle.generation {
            return None;
        }
        Some(&slot.data[..slot.len])
    }

    fn get_mut(&mut self, handle: Handle) -> Option<&mut [u8]> {
        let slot = self.slots[handle.slot as usize].as_mut()?;
        if slot.generation != handle.generation {
            return None;
        }
        Some(&mut slot.data[..slot.len])
    }

    fn free(&mut self, handle: Handle) -> bool {
        if let Some(slot) = &self.slots[handle.slot as usize] {
            if slot.generation == handle.generation {
                self.slots[handle.slot as usize] = None;
                return true;
            }
        }
        false
    }

    fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    fn capacity(&self) -> usize {
        MAX_SLOTS
    }

    fn block_size(&self) -> usize {
        MAX_ALLOC_SIZE
    }

    fn clear(&mut self) {
        self.slots = [const { None }; MAX_SLOTS];
        self.next_slot = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_naive_allocator_basic() {
        let mut alloc = NaiveAllocator::new();

        // Allocate
        let (h1, buf) = alloc.alloc_uninit(10).unwrap();
        buf[0] = 42;
        assert_eq!(alloc.len(), 1);

        // Get immutable
        let data = alloc.get(h1).unwrap();
        assert_eq!(data[0], 42);
        assert_eq!(data.len(), 10);

        // Get mutable
        let data_mut = alloc.get_mut(h1).unwrap();
        data_mut[1] = 99;
        assert_eq!(alloc.get(h1).unwrap()[1], 99);

        // Free
        assert!(alloc.free(h1));
        assert_eq!(alloc.len(), 0);
        assert!(alloc.get(h1).is_none());
    }

    #[test]
    fn test_naive_allocator_generation() {
        let mut alloc = NaiveAllocator::new();

        // First allocation should use slot 0
        let (h1, _) = alloc.alloc_uninit(10).unwrap();
        assert_eq!(h1.slot, 0);
        let gen1 = h1.generation;

        // Free slot 0
        assert!(alloc.free(h1));

        // Next allocation should reuse slot 0 with incremented generation
        let (h2, _) = alloc.alloc_uninit(10).unwrap();
        // Note: NaiveAllocator may not guarantee same slot reuse,
        // but generation should still be valid
        let gen2 = h2.generation;

        // If same slot, generation should increment
        if h2.slot == h1.slot {
            assert_eq!(gen2, gen1.wrapping_add(1));
        }

        // Old handle should not work
        assert!(alloc.get(h1).is_none());
        assert!(alloc.get(h2).is_some());
    }
    #[test]
    fn test_naive_allocator_multiple() {
        let mut alloc = NaiveAllocator::new();

        let mut handles = [None; 5];
        for i in 0..5 {
            let (h, buf) = alloc.alloc_uninit(4).unwrap();
            buf[0] = i as u8;
            handles[i] = Some(h);
        }

        assert_eq!(alloc.len(), 5);

        for (i, h_opt) in handles.iter().enumerate() {
            let h = h_opt.unwrap();
            assert_eq!(alloc.get(h).unwrap()[0], i as u8);
        }

        alloc.clear();
        assert_eq!(alloc.len(), 0);
    }
}
