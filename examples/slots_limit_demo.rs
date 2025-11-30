// Example demonstrating the SLOTS <= 255 compile-time check
//
// This example shows that TinySlabAllocator will produce a compile error
// if you try to use more than 255 slots.

use tinyalloc::prelude::*;

fn main() {
    // ✅ This works: 255 slots (max for u8 metadata)
    let mut alloc = TinySlabAllocator::<2040, 255>::new();
    println!("Created allocator with 255 slots");

    if let Some((handle, buf)) = alloc.alloc_uninit(8) {
        buf.copy_from_slice(b"Hello!!!");
        println!("Allocated 8 bytes successfully");
        alloc.free(handle);
    }

    // ❌ Uncomment the line below to see a compile error:
    // let _invalid = TinySlabAllocator::<8192, 256>::new();
    //
    // Error message will be:
    // "SLOTS must be <= 255 to fit in u8 metadata"
}
