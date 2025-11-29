//! Example demonstrating the prelude import

use rsalloc::prelude::*;

fn main() {
    // All common types available through prelude:
    // - Allocator trait
    // - Handle struct
    // - TinySlabAllocator (when feature = "tinyslab")
    // - ByteBuffer

    let mut alloc = TinySlabAllocator::<512, 64>::new();
    println!("Created TinySlabAllocator with {} slots", alloc.capacity());

    let _buf = ByteBuffer::new();
    println!("Created ByteBuffer"); // Demonstrate allocation
    if let Some((handle, slice)) = alloc.alloc_uninit(8) {
        slice.copy_from_slice(b"Testing!");
        println!(
            "Allocated handle: slot={}, gen={}",
            handle.slot, handle.generation
        );
        alloc.free(handle);
    }

    println!("\nPrelude makes imports easy: use rsalloc::prelude::*");
}
