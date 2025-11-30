//! Global Allocator Usage Example (Init Pattern)
//!
//! Demonstrates the init pattern for configuring the global allocator.
//! Users can choose from predefined configurations.

use tinyalloc::prelude::*;

fn main() {
    println!("=== Global Allocator ByteBuffer Usage (Init Pattern) ===\n");

    // STEP 1: Initialize the global allocator FIRST
    // This must happen before any ByteBuffer operations
    println!("Initializing global allocator (1KB, 32-byte blocks)...");
    GlobalAllocatorConfig::Slab1K32.init();

    // Get initial stats
    let alloc_stats = stats();
    println!("Initial allocator state:");
    println!("  Capacity: {} slots", alloc_stats.capacity);
    println!("  Used: {} slots", alloc_stats.used);
    println!("  Block size: {} bytes\n", alloc_stats.block_size);

    // STEP 2: Use ByteBuffer with simplified API
    let mut buf = ByteBuffer::new();

    // Append data using global allocator
    println!("Appending 'Hello, World!' to buffer...");
    buf.extend(b"Hello, World!").unwrap();
    println!("  Buffer length: {} bytes", buf.len());

    // Read data back
    print!("  Content: ");
    for byte in buf.bytes() {
        print!("{}", byte as char);
    }
    println!();

    let alloc_stats = stats();
    println!("  Allocator used: {} slots\n", alloc_stats.used);

    // Create another buffer
    let mut buf2 = ByteBuffer::new();
    buf2.extend(b"Simplified API!").unwrap();

    println!("Created second buffer:");
    println!("  Buffer length: {} bytes", buf2.len());
    print!("  Content: ");
    for byte in buf2.bytes() {
        print!("{}", byte as char);
    }
    println!();
    println!(
        "  Allocator used: {} slots\n",
        stats().used
    );

    // Combine buffers
    let mut buf3 = ByteBuffer::new();
    buf3.copy_from_global(&buf, 0..5).unwrap();
    buf3.extend(b" ").unwrap();
    buf3.copy_from_global(&buf2, 0..10).unwrap();

    println!("Combined buffer:");
    print!("  Content: ");
    for byte in buf3.bytes() {
        print!("{}", byte as char);
    }
    println!();
    println!(
        "  Allocator used: {} slots\n",
        stats().used
    );

    // Manual cleanup
    println!("Cleaning up buffers...");
    buf.clear();
    buf2.clear();
    buf3.clear();
    println!(
        "  Allocator used after cleanup: {} slots\n",
        stats().used
    );

    println!("✓ All memory manually freed!");
}
