#![no_std]

pub mod backend;
pub mod prelude;
pub mod utils;

pub fn version() -> &'static str {
    "0.1.0"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    pub slot: u16,
    pub generation: u8,
}

impl Handle {
    pub const fn new(slot: u16, generation: u8) -> Self {
        Self { slot, generation }
    }
    pub const fn parts(&self) -> (u16, u8) {
        (self.slot, self.generation)
    }
}

pub trait Allocator {
    fn alloc_uninit(&mut self, len: usize) -> Option<(Handle, &mut [u8])>;
    fn get(&self, handle: Handle) -> Option<&[u8]>;
    fn get_mut(&mut self, handle: Handle) -> Option<&mut [u8]>;
    fn free(&mut self, handle: Handle) -> bool;
    fn len(&self) -> usize;
    fn capacity(&self) -> usize;
    fn alloc(&mut self, data: &[u8]) -> Option<Handle> {
        let (h, buf) = self.alloc_uninit(data.len())?;
        buf.copy_from_slice(data);
        Some(h)
    }
    fn clear(&mut self);
}
