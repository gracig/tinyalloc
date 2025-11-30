//! Stream Processing with Global Allocator
//!
//! Demonstrates how easy it is to use stream operations without passing
//! the allocator explicitly - just use the `_global` methods.
//!
//! Run with: cargo run --example stream_global --features bytebuffer-stream

use tinyalloc::prelude::*;

fn main() {
    println!("=== Stream Processing with Global Allocator ===\n");

    // Initialize global allocator once
    GlobalAllocatorConfig::Slab1K32.init();

    let mut buf = ByteBuffer::new();

    // === Skip whitespace ===
    println!("1. Skip whitespace (no allocator arg!):");
    with_global_allocator(|alloc| {
        buf.write(alloc).extend(b"   \t  Hello World").unwrap();
    });
    let skipped = buf.skip_while_global(|b| b == b' ' || b == b'\t');
    println!("   Skipped: {}", skipped);
    println!("   First: '{}'", buf.peek_front_global().unwrap() as char);

    // === Consume until delimiter ===
    println!("\n2. Line parsing (no allocator arg!):");
    with_global_allocator(|alloc| {
        buf.write(alloc).clear();
        buf.write(alloc).extend(b"GET /api HTTP/1.1\r\n").unwrap();
    });
    if let Some(n) = buf.consume_until_global(b'\n') {
        println!("   Consumed line: {} bytes", n);
    }

    // === Peek ahead ===
    println!("\n3. Lookahead (no allocator arg!):");
    with_global_allocator(|alloc| {
        buf.write(alloc).clear();
        buf.write(alloc).extend(b"HTTP/1.1").unwrap();
    });
    print!("   Protocol: ");
    for i in 0..8 {
        if let Some(byte) = buf.peek_n_global(i) {
            print!("{}", byte as char);
        }
    }
    println!();

    // === Frame extraction ===
    println!("\n4. Frame extraction (no allocator arg!):");
    with_global_allocator(|alloc| {
        buf.write(alloc).clear();
        buf.write(alloc).extend(b"\x05Hello World").unwrap();
    });

    let mut len_buf = [0u8; 1];
    buf.copy_prefix_to_global(&mut len_buf);
    let frame_len = len_buf[0] as usize;

    let mut frame = [0u8; 16];
    let copied = buf.copy_prefix_to_global(&mut frame[..frame_len]);
    print!("   Frame: ");
    for i in 0..copied {
        print!("{}", frame[i] as char);
    }
    println!();

    // === Buffer transfer ===
    println!("\n5. Buffer transfer (no allocator arg!):");
    let mut src = ByteBuffer::new();
    let mut dst = ByteBuffer::new();

    with_global_allocator(|alloc| {
        src.write(alloc).extend(b"Source data").unwrap();
        dst.write(alloc).extend(b"Dest: ").unwrap();
    });

    let moved = src.drain_into_global(&mut dst, 6);
    println!("   Moved: {} bytes", moved);
    println!("   Src: {} bytes, Dst: {} bytes", src.len(), dst.len());

    // === Split buffer ===
    println!("\n6. Split buffer (no allocator arg!):");
    with_global_allocator(|alloc| {
        buf.write(alloc).clear();
        buf.write(alloc).extend(b"Hello:World").unwrap();
    });

    if let Some(pos) = buf.position_global(|b| b == b':') {
        if let Ok(first) = buf.split_off_front_global(pos) {
            println!("   First part: {} bytes", first.len());
            buf.remove_prefix_global(1); // Skip delimiter
            println!("   Second part: {} bytes", buf.len());
        }
    }

    println!("\n✓ All operations done without explicit allocator args!");
    with_global_allocator(|alloc| {
        println!(
            "  Allocator usage: {} / {} slots",
            alloc.len(),
            alloc.capacity()
        );
    });
}
