//! Example demonstrating ByteBuffer capacity with 2-byte metadata optimization
//!
//! This example shows how the bit-packed metadata allows more efficient use of memory.

use tinyalloc::prelude::*;

fn test_allocator<const CAPACITY: usize, const SLOTS: usize>(capacity_str: &str, slots: usize) {
    let mut alloc: TinySlabAllocator<CAPACITY, SLOTS> = TinySlabAllocator::new();

    let block_size = alloc.block_size();
    let layout = alloc.bit_layout();

    println!("Allocator Configuration:");
    println!("  Total capacity: {} bytes", capacity_str);
    println!("  Number of slots: {}", slots);
    println!("  Block size: {} bytes", block_size);
    println!();

    println!("Bit Layout (packed into u16):");
    println!(
        "  Slot bits: {} (max value: {})",
        layout.slot_bits,
        (1 << layout.slot_bits) - 1
    );
    println!(
        "  Gen bits:  {} (max value: {})",
        layout.gen_bits,
        (1 << layout.gen_bits) - 1
    );
    println!(
        "  Len bits:  {} (max value: {})",
        layout.len_bits,
        (1 << layout.len_bits) - 1
    );
    println!(
        "  Total: {} bits (fits in u16)",
        layout.slot_bits + layout.gen_bits + layout.len_bits
    );
    println!();

    println!("ByteBuffer Block Layout:");
    println!("  Metadata: 2 bytes (packed u16)");
    println!("  Data capacity per block: {} bytes", block_size - 2);
    println!(
        "  Total data capacity: {} × {} = {} bytes",
        slots,
        block_size - 2,
        slots * (block_size - 2)
    );
    println!();

    // Test by filling the buffer with maximum data
    let mut buf = ByteBuffer::new();

    // Fill with sequential bytes
    let mut write = buf.write(&mut alloc);
    let mut bytes_written = 0;

    // Write until we hit capacity
    for i in 0u8..=255 {
        match write.append(i) {
            Ok(_) => bytes_written += 1,
            Err(_) => break,
        }
    }

    // Try to write more data
    for i in 0u8..=255 {
        match write.append(i) {
            Ok(_) => bytes_written += 1,
            Err(_) => break,
        }
    }

    drop(write);

    println!("Actual Usage Test:");
    println!("  Bytes written: {}", bytes_written);
    println!("  Buffer length: {}", buf.len());
    println!("  Allocator slots used: {}", alloc.len());
    println!();

    // Calculate efficiency
    let theoretical_capacity = slots * (block_size - 2);
    let efficiency = (bytes_written as f32 / theoretical_capacity as f32) * 100.0;

    println!("Capacity Analysis:");
    println!("  Theoretical max: {} bytes", theoretical_capacity);
    println!("  Actually stored: {} bytes", bytes_written);
    println!("  Efficiency: {:.1}%", efficiency);
    println!();

    // Verify we can read back all the data
    let read = buf.read(&alloc);
    let mut read_count = 0;
    for _ in read.bytes() {
        read_count += 1;
    }

    println!("Data Integrity:");
    println!("  Bytes readable: {}", read_count);
    println!(
        "  Match written: {}",
        if read_count == bytes_written {
            "✓ YES"
        } else {
            "✗ NO"
        }
    );
    println!();

    // Show comparison with old 3-byte metadata
    let old_capacity_per_block = block_size.saturating_sub(3);
    let old_total = slots * old_capacity_per_block;
    let improvement = theoretical_capacity.saturating_sub(old_total);
    let improvement_pct = if old_total > 0 {
        (improvement as f32 / old_total as f32) * 100.0
    } else {
        0.0
    };

    println!("Comparison with 3-byte metadata:");
    println!("  Old capacity per block: {} bytes", old_capacity_per_block);
    println!("  Old total capacity: {} bytes", old_total);
    println!("  New total capacity: {} bytes", theoretical_capacity);
    println!(
        "  Space gained: {} bytes ({:.1}% improvement)",
        improvement, improvement_pct
    );
    println!();
}

fn main() {
    println!("=== ByteBuffer Capacity Demonstration ===\n");

    println!("TEST 1: 16-byte blocks (TinySlabAllocator<640, 40>)");
    println!("============================================================");
    test_allocator::<640, 40>("640", 40);

    println!("\n============================================================\n");

    println!("TEST 2: 8-byte blocks (TinySlabAllocator<688, 86>) - 512+ bytes data target");
    println!("============================================================");
    test_allocator::<688, 86>("688", 86);

    println!("\n============================================================\n");

    println!("TEST 3: 32-byte blocks (TinySlabAllocator<576, 18>) - 512+ bytes data target");
    println!("============================================================");
    test_allocator::<576, 18>("576", 18);
}
