//! Comprehensive ByteBuffer example demonstrating all key features

use rsalloc::prelude::*;

fn main() {
    println!("=== ByteBuffer Usage Examples ===\n");

    // Setup allocator with 64 slots of 12 bytes each
    let mut alloc = TinySlabAllocator::<768, 64>::new();

    // Example 1: Basic write and read
    basic_usage(&mut alloc);

    // Example 2: Copy between buffers
    copy_between_buffers(&mut alloc);

    // Example 3: Zero-copy move
    zero_copy_move(&mut alloc);

    // Example 4: Iterating over data
    iterate_data(&mut alloc);

    // Example 5: Max length enforcement
    max_length_example(&mut alloc);
}

fn basic_usage(alloc: &mut impl Allocator) {
    println!("1. Basic Write and Read:");

    let mut buf = ByteBuffer::new();

    // Write data
    buf.write(alloc).append(b'H').unwrap();
    buf.write(alloc).append(b'i').unwrap();
    buf.write(alloc).append(b'!').unwrap();

    println!("   Buffer length: {}", buf.len());

    // Read data
    print!("   Content: ");
    for byte in buf.read(alloc).bytes() {
        print!("{}", byte as char);
    }
    println!("\n");

    // Drain the buffer (zero-copy clear)
    let _ = buf.drain();
}

fn copy_between_buffers(alloc: &mut impl Allocator) {
    println!("2. Copy Between Buffers:");

    let mut src = ByteBuffer::new();
    let mut dst = ByteBuffer::new();

    // Fill source
    src.write(alloc).extend(b"Source Data").unwrap();

    println!("   Source length: {}", src.len());

    // Copy range to destination
    dst.copy_from(alloc, &src, 0..6).unwrap();

    print!("   Copied to dest: ");
    for byte in dst.read(alloc).bytes() {
        print!("{}", byte as char);
    }
    println!();

    println!("   Dest length: {}\n", dst.len());

    let _ = src.drain();
    let _ = dst.drain();
}

fn zero_copy_move(alloc: &mut impl Allocator) {
    println!("3. Zero-Copy Move:");

    let mut buf1 = ByteBuffer::new();

    // Write to first buffer
    buf1.write(alloc).extend(b"Move Me!").unwrap();

    println!("   buf1 length before: {}", buf1.len());

    // Zero-copy move to buf2
    let mut buf2 = ByteBuffer::new();
    buf2.move_from(&mut buf1);

    println!("   buf1 length after: {}", buf1.len());
    println!("   buf2 length after: {}", buf2.len());

    print!("   buf2 content: ");
    for byte in buf2.read(alloc).bytes() {
        print!("{}", byte as char);
    }
    println!("\n");

    let _ = buf2.drain();
}

fn iterate_data(alloc: &mut impl Allocator) {
    println!("4. Iterating Over Data:");

    let mut buf = ByteBuffer::new();
    buf.write(alloc).extend(b"Iterate!").unwrap();

    // Iterate by bytes
    print!("   Bytes: ");
    for (i, byte) in buf.read(alloc).bytes().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("{}", byte);
    }
    println!();

    // Iterate by chunks
    print!("   Chunks: ");
    for (i, chunk) in buf.read(alloc).chunks().enumerate() {
        if i > 0 {
            print!(" | ");
        }
        for &b in chunk {
            print!("{}", b as char);
        }
    }
    println!("\n");

    let _ = buf.drain();
}

fn max_length_example(alloc: &mut impl Allocator) {
    println!("5. Max Length Enforcement:");

    let mut buf = ByteBuffer::with_max_len(10);

    println!("   Max length: {:?}", buf.max_len());

    // Write within limit
    buf.write(alloc).extend(b"Short").unwrap();

    println!("   Current length: {} (within limit)", buf.len());

    // Try to exceed limit
    match buf.write(alloc).extend(b"TooLong") {
        Ok(_) => println!("   Write succeeded"),
        Err(_) => println!("   Hit max length limit!"),
    }

    println!("   Final length: {}\n", buf.len());

    let _ = buf.drain();
}
