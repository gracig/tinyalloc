//! ByteBuffer Stream Processing Example
//!
//! Demonstrates advanced stream processing operations when the `bytebuffer-stream` feature is enabled.
//! Shows how to efficiently parse and manipulate data streams using predicates, delimiters, and buffer operations.
//!
//! Run with: cargo run --example bytebuffer_stream --features bytebuffer-stream

use tinyalloc::prelude::*;

fn main() {
    println!("=== ByteBuffer Stream Processing Operations ===\n");

    // Initialize allocator
    let mut alloc = TinySlabAllocator::<1024, 32>::new();

    // === 1. Lookahead with peek_n ===
    println!("1. Lookahead parsing with peek_n:");
    let mut buf = ByteBuffer::new();
    buf.write(&mut alloc)
        .extend(b"HTTP/1.1 200 OK\r\n")
        .unwrap();

    print!("   Protocol: ");
    for i in 0..8 {
        if let Some(byte) = buf.peek_n(&alloc, i) {
            print!("{}", byte as char);
        }
    }
    println!("\n   Buffer still intact: {} bytes", buf.len());

    // === 2. Skip whitespace with skip_while ===
    println!("\n2. Skip leading whitespace:");
    buf.write(&mut alloc).clear();
    buf.write(&mut alloc)
        .extend(b"    \t  Hello World")
        .unwrap();
    println!("   Before: {} bytes total", buf.len());

    let skipped = buf.skip_while(&mut alloc, |b| b == b' ' || b == b'\t');
    println!("   Skipped {} whitespace chars", skipped);
    println!("   After: {} bytes remaining", buf.len());

    // Verify first byte is 'H'
    if let Some(first) = buf.peek_front(&alloc) {
        println!("   First byte after skip: '{}'", first as char);
    }

    // === 3. Line-oriented parsing with consume_until ===
    println!("\n3. Line-oriented parsing:");
    buf.write(&mut alloc).clear();
    buf.write(&mut alloc)
        .extend(b"GET /api/data HTTP/1.1\r\nHost: example.com\r\n\r\n")
        .unwrap();

    if let Some(consumed) = buf.consume_until(&mut alloc, b'\n') {
        println!("   Consumed first line: {} bytes", consumed);
        println!("   Remaining: {} bytes", buf.len());
    }

    if let Some(consumed) = buf.consume_until(&mut alloc, b'\n') {
        println!("   Consumed second line: {} bytes", consumed);
    }

    // === 4. Fixed-size frame extraction with copy_prefix_to ===
    println!("\n4. Fixed-size frame extraction:");
    buf.write(&mut alloc).clear();
    buf.write(&mut alloc)
        .extend(b"\x05\x00Hello more data")
        .unwrap();

    // Read length prefix (1 byte)
    let mut len_buf = [0u8; 1];
    buf.copy_prefix_to(&mut alloc, &mut len_buf);
    let payload_len = len_buf[0] as usize;
    println!("   Frame length: {}", payload_len);

    // Skip frame type (1 byte for demo)
    buf.remove_prefix(&mut alloc, 1);

    // Extract payload into fixed-size buffer
    let mut payload = [0u8; 16];
    let copied = buf.copy_prefix_to(&mut alloc, &mut payload[..payload_len.min(16)]);
    print!("   Payload: '");
    for i in 0..copied {
        print!("{}", payload[i] as char);
    }
    println!("' ({} bytes)", copied);
    println!("   Remaining: {} bytes", buf.len());

    // === 5. Message splitting with split_off_front ===
    println!("\n5. Message splitting:");
    buf.write(&mut alloc).clear();
    buf.write(&mut alloc)
        .extend(b"Message1:Message2:Message3")
        .unwrap();

    // Find first delimiter
    if let Some(pos) = buf.position(&alloc, |b| b == b':') {
        println!("   Found delimiter at position {}", pos);

        // Split off first message
        if let Ok(msg1) = buf.split_off_front(&mut alloc, pos) {
            println!("   Message 1 length: {} bytes", msg1.len());
            println!(
                "   First char: '{}'",
                msg1.peek_n(&alloc, 0).unwrap() as char
            );

            // Remove delimiter
            buf.remove_prefix(&mut alloc, 1);
            println!("   Remaining length: {} bytes", buf.len());
            println!(
                "   First char: '{}'",
                buf.peek_front(&alloc).unwrap() as char
            );
        }
    }

    // === 6. Buffer transfer with drain_into ===
    println!("\n6. Zero-copy buffer transfer:");
    let mut src = ByteBuffer::new();
    let mut dst = ByteBuffer::new();

    src.write(&mut alloc)
        .extend(b"Producer data stream")
        .unwrap();
    dst.write(&mut alloc).extend(b"[Header] ").unwrap();

    println!("   Source: {} bytes, Dest: {} bytes", src.len(), dst.len());

    // Move 8 bytes from source to destination
    let moved = src.drain_into(&mut alloc, &mut dst, 8);
    println!("   Moved {} bytes", moved);
    println!("   Source: {} bytes, Dest: {} bytes", src.len(), dst.len());
    println!(
        "   Dest first char: '{}'",
        dst.peek_front(&alloc).unwrap() as char
    );

    // === 7. Complex parsing pattern ===
    println!("\n7. Complex parsing (CSV-like):");
    buf.write(&mut alloc).clear();
    buf.write(&mut alloc)
        .extend(b"  field1  , field2, field3 \n")
        .unwrap();

    let mut field_count = 0;

    loop {
        // Skip leading whitespace
        buf.skip_while(&mut alloc, |b| b == b' ');

        if buf.is_empty() {
            break;
        }

        // Count field length
        let mut field_len = 0;
        while let Some(byte) = buf.peek_n(&alloc, field_len) {
            if byte == b',' || byte == b'\n' {
                break;
            }
            field_len += 1;
        }

        // Trim trailing spaces from count
        while field_len > 0 {
            if let Some(byte) = buf.peek_n(&alloc, field_len - 1) {
                if byte != b' ' {
                    break;
                }
            }
            field_len -= 1;
        }

        if field_len > 0 {
            field_count += 1;
            println!("   Field {}: {} bytes", field_count, field_len);

            // Consume the field
            buf.remove_prefix(&mut alloc, field_len);

            // Skip trailing spaces that were trimmed
            buf.skip_while(&mut alloc, |b| b == b' ');
        }

        // Skip delimiter
        if let Some(delim) = buf.peek_front(&alloc) {
            if delim == b',' || delim == b'\n' {
                buf.pop_front(&mut alloc);
            }
        }

        if buf.is_empty() {
            break;
        }
    }
    println!("\n✓ All stream operations completed successfully!");
    println!(
        "  Allocator usage: {} / {} slots",
        alloc.len(),
        alloc.capacity()
    );
}
