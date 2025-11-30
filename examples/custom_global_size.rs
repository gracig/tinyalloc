//! Custom Global Allocator Size Example
//!
//! Demonstrates how users can configure their own allocator size
//! using the init pattern. This shows the flexibility of the
//! StaticCell-based initialization approach.

use tinyalloc::prelude::*;

// Define a custom allocator type with larger capacity
// 4KB RAM with 128 slots (32-byte blocks)
type CustomAllocator = TinySlabAllocator<4096, 128>;

fn main() {
    println!("=== Custom Global Allocator Size Example ===\n");

    // Initialize with CUSTOM size (4KB instead of default 2KB)
    println!("Initializing custom allocator (4KB, 128 slots)...");
    init_global_allocator(CustomAllocator::new());

    // Verify the custom configuration
    let alloc_stats = stats();
    println!("Custom allocator initialized:");
    println!("  Capacity: {} slots", alloc_stats.capacity);
    println!("  Block size: {} bytes", alloc_stats.block_size);
    println!(
        "  Total memory: {} bytes\n",
        alloc_stats.capacity * alloc_stats.block_size
    );

    // Now we can allocate more data than the default would allow
    let mut buf = ByteBuffer::new();

    // Create larger data (would fail with default 2KB allocator in tests)
    let large_data = vec![b'X'; 100];
    buf.extend(&large_data).unwrap();

    println!("Allocated {} bytes successfully!", buf.len());
    println!("  Used slots: {}", stats().used);

    // Create multiple buffers to show capacity
    let mut buffers = Vec::new();
    for i in 0..5 {
        let mut b = ByteBuffer::new();
        b.extend(format!("Buffer {}", i).as_bytes()).unwrap();
        buffers.push(b);
    }

    println!("\nCreated {} additional buffers", buffers.len());
    println!(
        "  Total used slots: {}/{}",
        stats().used,
        stats().capacity
    );

    // Cleanup
    buf.clear();
    for mut b in buffers {
        b.clear();
    }

    println!("\n✓ All buffers cleaned up!");
    println!("  Final used slots: {}", stats().used);

    println!("\n=== Key Takeaway ===");
    println!("Users can define their own allocator size in their main()");
    println!("before any ByteBuffer operations, without editing library source!");
}
