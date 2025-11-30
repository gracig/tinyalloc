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
    with_global_allocator(|alloc| {
        println!("Initial allocator state:");
        println!("  Capacity: {} slots", alloc.capacity());
        println!("  Used: {} slots", alloc.len());
        println!("  Block size: {} bytes\n", alloc.block_size());
    });

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

    with_global_allocator(|alloc| {
        println!("  Allocator used: {} slots\n", alloc.len());
    });

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
    with_global_allocator(|alloc| {
        println!("  Allocator used: {} slots\n", alloc.len());
    });

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
    with_global_allocator(|alloc| {
        println!("  Allocator used: {} slots\n", alloc.len());
    });

    // Manual cleanup
    println!("Cleaning up buffers...");
    buf.clear();
    buf2.clear();
    buf3.clear();
    with_global_allocator(|alloc| {
        println!("  Allocator used after cleanup: {} slots\n", alloc.len());
    });

    println!("✓ All memory manually freed!");
}
