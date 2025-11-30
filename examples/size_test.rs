#![no_std]
#![no_main]

use core::panic::PanicInfo;
use tinyalloc::prelude::*;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

fn main() {
    GlobalAllocatorConfig::Slab1K32.init();
    with_global_allocator(|alloc| {
        let (handle, buf) = alloc.alloc_uninit(4).unwrap();
        buf.copy_from_slice(b"test");
        let _ = alloc.get(handle);
        alloc.free(handle);
    });
}
