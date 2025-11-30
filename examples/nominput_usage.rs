//! Example demonstrating nom parser integration with ByteBuffer
//!
//! This example shows how to use ByteBuffer as input for nom parsers
//! when the `nominput` feature is enabled.
//!
//! Run with: cargo run --example nominput_usage --features nominput

use tinyalloc::prelude::*;

fn main() {
    println!("=== Nom Parser Integration with ByteBuffer ===\n");
    // Initialize allocator
    GlobalAllocatorConfig::Slab1K32.init();

    // Create and populate a ByteBuffer
    let mut buf = ByteBuffer::new();
    buf.extend(b"Hello, World! 12345").unwrap();

    // ByteBuffer now implements nom::Input directly!
    println!("Buffer content: {:?}", buf.bytes().collect::<Vec<_>>());

    // Check the length using nom's Input trait
    let len = buf.input_len();
    println!("Input length: {}", len);
    assert_eq!(len, 19);

    // Use nom's take operation to get first N bytes
    let taken = buf.take(5);
    println!("Taken first 5 bytes, length: {}", taken.len());
    assert_eq!(taken.len(), 5);

    // Use take_split to split at position
    let (_remaining, first) = buf.take_split(5);
    println!("After take_split(5), first part length: {}", first.len());
    assert_eq!(first.len(), 5);

    // For actual byte-by-byte parsing, use the bytes() iterator
    print!("First 6 bytes: ");
    for (i, byte) in buf.bytes().enumerate() {
        if i >= 6 {
            break;
        }
        print!("{} ", byte as char);
    }
    println!();

    // slice_index validation
    match buf.slice_index(10) {
        Ok(idx) => {
            println!("slice_index(10) = Ok({})", idx);
            assert_eq!(idx, 10);
        }
        Err(_) => panic!("Should have succeeded"),
    }

    match buf.slice_index(100) {
        Ok(_) => panic!("Should have failed"),
        Err(needed) => {
            println!("slice_index(100) = Err(Needed({:?}))", needed);
        }
    }

    // Test position() - finds first byte matching predicate
    // This uses global allocator when available (nominput feature enables it)
    let comma_pos = nom::Input::position(&buf, |b| b == b',');
    println!("Position of ',' = {:?}", comma_pos);
    assert_eq!(comma_pos, Some(5));

    let exclaim_pos = nom::Input::position(&buf, |b| b == b'!');
    println!("Position of '!' = {:?}", exclaim_pos);
    assert_eq!(exclaim_pos, Some(12));

    // Test iter_elements - iterate over all bytes
    println!("\nTesting iter_elements:");
    print!("  All bytes via iter_elements: ");
    let mut iter = nom::Input::iter_elements(&buf);
    let mut count = 0;
    while let Some(byte) = iter.next() {
        if count < 13 {
            print!("{}", byte as char);
        }
        count += 1;
    }
    println!(" ({} bytes)", count);
    assert_eq!(count, 19);

    // Test iter_indices - iterate with indices
    println!("\nTesting iter_indices:");
    let indices: Vec<_> = nom::Input::iter_indices(&buf).take(7).collect();
    print!("  First 7 with indices: ");
    for (idx, byte) in &indices {
        print!("[{}]={} ", idx, *byte as char);
    }
    println!();
    assert_eq!(indices.len(), 7);
    assert_eq!(indices[0], (0, b'H'));
    assert_eq!(indices[6], (6, b' ')); // Index 6 is the space after comma

    // Test deque functionality (requires bytebuffer-deque feature)
    #[cfg(feature = "bytebuffer-deque")]
    {
        println!("\nTesting deque functionality:");

        // Peek at front (immutable, doesn't need mut)
        let front = with_global_allocator(|alloc| buf.peek_front(alloc));
        println!("  Peek front: {:?}", front.map(|b| b as char));
        assert_eq!(front, Some(b'H'));

        // Pop some bytes
        let (popped1, popped2) = with_global_allocator(|alloc| {
            let p1 = buf.pop_front(alloc).unwrap();
            let p2 = buf.pop_front(alloc).unwrap();
            (p1, p2)
        });
        println!("  Popped: {} {}", popped1 as char, popped2 as char);
        assert_eq!(popped1, b'H');
        assert_eq!(popped2, b'e');
        assert_eq!(buf.len(), 17); // 19 - 2

        // Remove prefix (efficient bulk removal)
        let removed = with_global_allocator(|alloc| buf.remove_prefix(alloc, 5)); // Remove "llo, "
        println!("  Removed {} bytes via remove_prefix", removed);
        assert_eq!(removed, 5);
        assert_eq!(buf.len(), 12); // Should have "World! 12345"

        // Verify remaining content
        print!("  Remaining after removal: ");
        for byte in nom::Input::iter_elements(&buf).take(12) {
            print!("{}", byte as char);
        }
        println!();

        println!("  ✓ Deque functionality works!");
    }
    #[cfg(not(feature = "bytebuffer-deque"))]
    {
        println!("\nNote: Deque functionality requires 'bytebuffer-deque' feature");
    }

    println!("\n✓ All nom::Input trait methods work correctly, including iterators!");
}
