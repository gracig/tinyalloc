//! Example: ByteBufferDeque with Global Allocator in Interrupt-like Context
//!
//! This demonstrates a typical embedded pattern where:
//! - A global deque is shared between main code and interrupt handler
//! - Interrupt handler pushes data (simulating sensor/UART/etc)
//! - Main loop consumes and processes data
//! - Single-threaded design (no mutexes needed)

use tinyalloc::prelude::*;

// Global shared deque - uses the built-in GlobalDeque wrapper
static SHARED_DEQUE: GlobalDeque = GlobalDeque::new();

/// Simulates an interrupt handler that receives data
/// In real embedded system, this would be called by hardware interrupt
fn interrupt_handler(byte: u8) {
    println!(
        "  [INTERRUPT] Received byte: 0x{:02X} ('{}')",
        byte, byte as char
    );

    // Push to shared deque using global allocator (simplified API)
    match SHARED_DEQUE.push(byte) {
        Ok(_) => println!(
            "  [INTERRUPT] Pushed to deque (len: {})",
            SHARED_DEQUE.len()
        ),
        Err(e) => println!("  [INTERRUPT] Failed to push: {:?}", e),
    }
}

/// Main loop that processes data from the deque
fn main_loop_process() {
    let available = SHARED_DEQUE.len();
    if available > 0 {
        println!("  [MAIN] Processing {} bytes from deque...", available);

        // Process all available bytes
        while let Some(byte) = SHARED_DEQUE.pop() {
            print!("  [MAIN] Consumed: 0x{:02X} ('{}')", byte, byte as char);

            // Simulate processing
            if byte.is_ascii_graphic() || byte == b' ' {
                println!(" -> valid ASCII");
            } else {
                println!(" -> control/special");
            }
        }

        println!("  [MAIN] Deque now empty (len: {})", SHARED_DEQUE.len());
    }
}

fn main() {
    println!("=== ByteBufferDeque with Global Allocator (Interrupt Pattern) ===\n");

    // Initialize global allocator
    println!("Initializing global allocator (512 bytes, 16-byte blocks)...");
    GlobalAllocatorConfig::Slab512b16.init();

    with_global_allocator(|alloc| {
        println!("  Capacity: {} slots", alloc.capacity());
        println!("  Block size: {} bytes", alloc.block_size());
    });
    println!();

    // Simulate embedded system main loop
    println!("--- Starting Main Loop ---\n");

    // Cycle 1: Interrupts arrive
    println!("Cycle 1: Interrupts arrive");
    interrupt_handler(b'H');
    interrupt_handler(b'e');
    interrupt_handler(b'l');
    interrupt_handler(b'l');
    interrupt_handler(b'o');
    println!();

    // Main loop processes
    println!("Cycle 1: Main loop processes");
    main_loop_process();
    println!();

    // Cycle 2: More interrupts
    println!("Cycle 2: More interrupts arrive");
    interrupt_handler(b' ');
    interrupt_handler(b'W');
    interrupt_handler(b'o');
    interrupt_handler(b'r');
    interrupt_handler(b'l');
    interrupt_handler(b'd');
    println!();

    // Cycle 3: Process partial data
    println!("Cycle 3: Main loop processes 3 bytes");
    for _ in 0..3 {
        if let Some(byte) = SHARED_DEQUE.pop() {
            println!("  [MAIN] Consumed: 0x{:02X} ('{}')", byte, byte as char);
        }
    }
    println!("  [MAIN] Remaining in deque: {}", SHARED_DEQUE.len());
    println!();

    // Cycle 4: More interrupts while data still in deque
    println!("Cycle 4: More interrupts (mixed with pending data)");
    interrupt_handler(b'!');
    interrupt_handler(0x0A); // newline
    println!();

    // Final processing
    println!("Cycle 5: Main loop processes all remaining data");
    main_loop_process();
    println!();

    // Show final statistics
    println!("--- Final Statistics ---");
    SHARED_DEQUE.with_mut(|deque| {
        println!("Deque length: {}", deque.len());
    });

    with_global_allocator(|alloc| {
        println!(
            "Allocator used: {} / {} slots",
            alloc.len(),
            alloc.capacity()
        );
    });

    println!("\n=== Pattern Demonstrated ===");
    println!("✓ Global allocator initialized once");
    println!("✓ Shared deque between interrupt and main");
    println!("✓ Producer (interrupt) pushes data");
    println!("✓ Consumer (main) pops and processes data");
    println!("✓ No mutex needed (single-threaded)");
    println!("✓ Zero-copy, efficient memory usage");
}
