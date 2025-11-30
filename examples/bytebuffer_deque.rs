//! ByteBuffer Deque Operations Example
//!
//! Demonstrates ByteBuffer's deque capabilities when the `bytebuffer-deque` feature is enabled.
//! Shows how to efficiently consume data from the front while appending to the back.
//!
//! Run with: cargo run --example bytebuffer_deque --features bytebuffer-deque

use tinyalloc::prelude::*;

fn main() {
    println!("=== ByteBuffer Deque Operations ===\n");

    // Initialize allocator
    let mut alloc = TinySlabAllocator::<1024, 32>::new();
    let mut buf = ByteBuffer::new();

    // === Basic FIFO Operations ===
    println!("1. Basic FIFO (First-In-First-Out):");
    buf.write(&mut alloc).extend(b"ABCDE").unwrap();
    println!("   Initial buffer: {:?} bytes", buf.len());

    // Peek without consuming
    if let Some(front) = buf.peek_front(&alloc) {
        println!("   Peek front: {}", front as char);
    }

    // Pop from front
    let first = buf.pop_front(&mut alloc).unwrap();
    let second = buf.pop_front(&mut alloc).unwrap();
    println!(
        "   Popped: {}, {} (length now: {})",
        first as char,
        second as char,
        buf.len()
    );

    // === Efficient Bulk Removal ===
    println!("\n2. Bulk removal with remove_prefix:");
    buf.write(&mut alloc).extend(b"FGHIJ").unwrap();
    println!("   Added more data, length: {}", buf.len());

    let removed = buf.remove_prefix(&mut alloc, 3);
    println!("   Removed {} bytes, length now: {}", removed, buf.len());

    // === Stream Processing Pattern ===
    println!("\n3. Stream processing (parse and consume):");
    buf.write(&mut alloc).extend(b"Hello, World!").unwrap();
    println!("   Stream buffer length: {}", buf.len());

    // Simulate parsing first word
    let mut word_len = 0;
    loop {
        if let Some(byte) = buf.peek_front(&alloc) {
            if byte == b',' || byte == b' ' {
                break;
            }
            buf.pop_front(&mut alloc);
            word_len += 1;
        } else {
            break;
        }
    }
    println!("   Parsed word length: {} bytes", word_len);
    println!("   Remaining in buffer: {} bytes", buf.len());

    // Skip delimiter and space
    buf.remove_prefix(&mut alloc, 2);

    // === Memory Efficiency ===
    println!("\n4. Memory efficiency (chunk freeing):");
    buf.write(&mut alloc).clear();

    // Fill first chunk completely
    let block_size = alloc.block_size();
    let usable = block_size - 2; // Subtract metadata
    for i in 0..usable {
        buf.write(&mut alloc).append(i as u8).unwrap();
    }
    println!(
        "   Filled first chunk: {} bytes, allocator usage: {} slots",
        buf.len(),
        alloc.len()
    );

    // Trigger second chunk
    buf.write(&mut alloc).append(99).unwrap();
    println!(
        "   Added 1 more byte: {} bytes, allocator usage: {} slots",
        buf.len(),
        alloc.len()
    );

    // Consume entire first chunk
    buf.remove_prefix(&mut alloc, usable);
    println!(
        "   Removed first chunk: {} bytes, allocator usage: {} slots",
        buf.len(),
        alloc.len()
    );
    println!("   ✓ First chunk automatically freed!");

    // === Combined Operations ===
    println!("\n5. Combined read/write operations:");
    buf.write(&mut alloc).clear();
    buf.write(&mut alloc).extend(b"Producer").unwrap();

    for _ in 0..3 {
        if let Some(byte) = buf.pop_front(&mut alloc) {
            print!("   Consumed: {}, ", byte as char);
        }
        buf.write(&mut alloc).append(b'X').unwrap();
        println!("length: {}", buf.len());
    }

    println!("\n✓ All deque operations completed successfully!");
    println!("  Final buffer length: {}", buf.len());
    println!("  Allocator usage: {} slots", alloc.len());
}
