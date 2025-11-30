//! Global allocator for use with ByteBuffer
//!
//! # Single-Threaded Design
//!
//! This global allocator is **NOT thread-safe** by design. It uses `UnsafeCell`
//! instead of `Mutex` for zero-cost abstraction in no_std embedded environments.
//!
//! ## Important Constraints:
//! - **Single-threaded only** - Must not be accessed from multiple threads
//! - **No concurrent access** - All operations must be sequential
//! - **Tests require `--test-threads=1`** - Parallel tests would corrupt state
//!
//! This is the correct design for embedded systems where:
//! - Applications run single-threaded (no OS threading)
//! - Deterministic performance is required (no mutex overhead)
//! - Memory is limited and predictable
//!
//! ## Automatic Default Initialization
//!
//! The global allocator **automatically initializes** with [`GlobalAllocatorConfig::Slab256b8`]
//! (256 bytes, 8 slots, 32-byte blocks) if not explicitly configured. This means:
//!
//! - **No panic on first use** - Always safe to call `with_global_allocator()`
//! - **Zero configuration** - Works out of the box for simple use cases
//! - **Predictable behavior** - Default is suitable for embedded environments
//!
//! ```rust,ignore
//! use tinyalloc::global::with_global_allocator;
//! use tinyalloc::utils::bytebuffer::ByteBuffer;
//!
//! fn main() {
//!     // No init needed - auto-initializes with Slab256b8
//!     let mut buf = ByteBuffer::new();
//!     buf.extend(b"Hello").unwrap(); // Works immediately
//! }
//! ```
//!
//! ## User Configuration (Optional)
//!
//! For custom memory requirements, explicitly initialize before first use:
//!
//! ```rust,ignore
//! use tinyalloc::global::GlobalAllocatorConfig;
//! use tinyalloc::utils::bytebuffer::ByteBuffer;
//!
//! fn main() {
//!     // Option 1: Use predefined configuration
//!     GlobalAllocatorConfig::Slab1K32.init();
//!     
//!     // Option 2: Custom allocator
//!     // use tinyalloc::global::init_global_allocator;
//!     // use tinyalloc::backend::tinyslab::TinySlabAllocator;
//!     // init_global_allocator(TinySlabAllocator::<2048, 64>::new());
//!     
//!     // Now use ByteBuffer with your configured allocator
//!     let mut buf = ByteBuffer::new();
//!     buf.extend(b"Hello").unwrap();
//! }
//! ```

use crate::backend::tinyslab::TinySlabAllocator;
use crate::{Allocator, Handle};
use core::cell::UnsafeCell;

/// Predefined allocator configurations
#[derive(Debug, Clone, Copy)]
pub enum GlobalAllocatorConfig {
    /// 256 bytes total, 32-byte blocks (8 slots)
    Slab256b32,
    /// 256 bytes total, 16-byte blocks (16 slots)
    Slab256b16,
    /// 256 bytes total, 8-byte blocks (32 slots)
    Slab256b8,
    /// 512 bytes total, 32-byte blocks (16 slots)
    Slab512b32,
    /// 512 bytes total, 16-byte blocks (32 slots)
    Slab512b16,
    /// 512 bytes total, 8-byte blocks (64 slots)  
    Slab512b8,
    /// 768 bytes total, 32-byte blocks (24 slots)
    Slab768b32,
    /// 768 bytes total, 16-byte blocks (48 slots)
    Slab768b16,
    /// 768 bytes total, 8-byte blocks (96 slots)
    Slab768b8,
    /// 1KB total, 32-byte blocks (32 slots)
    Slab1K32,
    /// 1KB total, 16-byte blocks (64 slots)
    Slab1K16,
    /// 1KB total, 8-byte blocks (128 slots)
    Slab1K8,
}

impl GlobalAllocatorConfig {
    /// Initialize the global allocator with this configuration
    ///
    /// Creates a static allocator internally for the selected configuration
    pub fn init(self) {
        macro_rules! init_static {
            ($size:expr, $slots:expr) => {{
                static mut ALLOC: TinySlabAllocator<$size, $slots> = TinySlabAllocator::new();
                unsafe {
                    let alloc_ptr = core::ptr::addr_of_mut!(ALLOC);
                    init_global_allocator(&mut *alloc_ptr)
                }
            }};
        }
        match self {
            GlobalAllocatorConfig::Slab256b32 => init_static!(256, 8),
            GlobalAllocatorConfig::Slab256b16 => init_static!(256, 16),
            GlobalAllocatorConfig::Slab256b8 => init_static!(256, 32),
            GlobalAllocatorConfig::Slab512b32 => init_static!(512, 16),
            GlobalAllocatorConfig::Slab512b16 => init_static!(512, 32),
            GlobalAllocatorConfig::Slab512b8 => init_static!(512, 64),
            GlobalAllocatorConfig::Slab768b32 => init_static!(768, 24),
            GlobalAllocatorConfig::Slab768b16 => init_static!(768, 48),
            GlobalAllocatorConfig::Slab768b8 => init_static!(768, 96),
            GlobalAllocatorConfig::Slab1K32 => init_static!(1024, 32),
            GlobalAllocatorConfig::Slab1K16 => init_static!(1024, 64),
            GlobalAllocatorConfig::Slab1K8 => init_static!(1024, 128),
        }
    }
}

/// Type-erased global allocator storage
struct GlobalStorage {
    /// Pointer to user's allocator (stored as dyn Allocator trait object)
    allocator_ptr: UnsafeCell<Option<&'static mut dyn Allocator>>,
}

unsafe impl Sync for GlobalStorage {}

impl GlobalStorage {
    const fn new() -> Self {
        Self {
            allocator_ptr: UnsafeCell::new(None),
        }
    }

    fn init<A: Allocator + 'static>(&self, allocator: &'static mut A) {
        unsafe {
            *self.allocator_ptr.get() = Some(allocator);
        }
    }
}

static GLOBAL: GlobalStorage = GlobalStorage::new();

/// Initialize the global allocator with a custom TinySlabAllocator configuration
///
/// This is **optional** - if not called, the allocator auto-initializes with
/// [`GlobalAllocatorConfig::Slab256b8`] on first use. Call this to customize
/// memory requirements for your application.
///
/// # Example
/// ```rust,ignore
/// use tinyalloc::global::init_global_allocator;
/// use tinyalloc::backend::tinyslab::TinySlabAllocator;
///
/// static mut MY_ALLOCATOR: TinySlabAllocator<4096, 128> = TinySlabAllocator::new();
///
/// fn main() {
///     // User must provide a static allocator
///     unsafe { init_global_allocator(&mut MY_ALLOCATOR); }
/// }
/// ```
pub fn init_global_allocator<A: Allocator + 'static>(allocator: &'static mut A) {
    GLOBAL.init(allocator);
}

/// Wrapper type that implements Allocator trait by forwarding to global allocator
/// This allows ByteBuffer to work with the global allocator
pub struct GlobalAllocatorRef;

impl Allocator for GlobalAllocatorRef {
    fn alloc(&mut self, data: &[u8]) -> Option<Handle> {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref_mut()
                .unwrap()
                .alloc(data)
        }
    }

    fn alloc_uninit(&mut self, len: usize) -> Option<(Handle, &mut [u8])> {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref_mut()
                .unwrap()
                .alloc_uninit(len)
        }
    }

    fn free(&mut self, handle: Handle) -> bool {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref_mut()
                .unwrap()
                .free(handle)
        }
    }

    fn get(&self, handle: Handle) -> Option<&[u8]> {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref()
                .unwrap()
                .get(handle)
        }
    }

    fn get_mut(&mut self, handle: Handle) -> Option<&mut [u8]> {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref_mut()
                .unwrap()
                .get_mut(handle)
        }
    }

    fn len(&self) -> usize {
        unsafe { (*GLOBAL.allocator_ptr.get()).as_deref().unwrap().len() }
    }

    fn capacity(&self) -> usize {
        unsafe { (*GLOBAL.allocator_ptr.get()).as_deref().unwrap().capacity() }
    }

    fn block_size(&self) -> usize {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref()
                .unwrap()
                .block_size()
        }
    }

    fn bit_layout(&self) -> crate::BitLayout {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref()
                .unwrap()
                .bit_layout()
        }
    }

    fn clear(&mut self) {
        unsafe {
            (*GLOBAL.allocator_ptr.get())
                .as_deref_mut()
                .unwrap()
                .clear()
        }
    }
}

/// Execute a closure with a reference to the global allocator
/// This provides compatibility with ByteBuffer's write/read API
///
/// If the global allocator has not been initialized, it will automatically
/// initialize with `GlobalAllocatorConfig::Slab256b8` as the default configuration.
#[inline]
pub fn with_global_allocator<F, R>(f: F) -> R
where
    F: FnOnce(&mut GlobalAllocatorRef) -> R,
{
    // Auto-initialize with default if not already initialized
    unsafe {
        if (*GLOBAL.allocator_ptr.get()).is_none() {
            GlobalAllocatorConfig::Slab256b8.init();
        }
    }
    f(&mut GlobalAllocatorRef)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::tinyslab::TinySlabAllocator;

    #[test]
    fn test_direct_alloc() {
        static mut ALLOC: TinySlabAllocator<2048, 64> = TinySlabAllocator::new();
        // Init
        unsafe {
            init_global_allocator(&mut *(core::ptr::addr_of_mut!(ALLOC)));
        }

        with_global_allocator(|alloc| {
            assert_eq!(alloc.capacity(), 64);
            assert_eq!(alloc.block_size(), 32);
            assert_eq!(alloc.len(), 0);

            // Try to allocate directly
            let result = alloc.alloc(b"test");
            assert!(result.is_some(), "Allocation should succeed");

            let handle = result.unwrap();
            assert_eq!(alloc.get(handle), Some(&b"test"[..]));
            assert_eq!(alloc.len(), 1);

            assert!(alloc.free(handle));
            assert_eq!(alloc.len(), 0);
        });
    }

    #[test]
    #[cfg(feature = "bytebuffer")]
    fn test_bytebuffer_with_global() {
        use crate::utils::bytebuffer::ByteBuffer;

        static mut ALLOC: TinySlabAllocator<2048, 64> = TinySlabAllocator::new();
        // Init
        unsafe {
            init_global_allocator(&mut *(core::ptr::addr_of_mut!(ALLOC)));
        }

        with_global_allocator(|alloc| {
            alloc.clear();

            // Create buffer and try to append
            let mut buf = ByteBuffer::new();
            let result = buf.append(42);

            assert!(result.is_ok(), "ByteBuffer append should succeed");
            assert_eq!(buf.len(), 1);
        });
    }

    #[test]
    fn test_init_custom_size() {
        static mut ALLOC: TinySlabAllocator<4096, 128> = TinySlabAllocator::new();
        // Initialize with custom 4KB allocator
        unsafe {
            init_global_allocator(&mut *(core::ptr::addr_of_mut!(ALLOC)));
        }

        with_global_allocator(|alloc| {
            // Verify configuration
            assert_eq!(alloc.capacity(), 128);
            assert_eq!(alloc.block_size(), 32);

            // Test allocation
            alloc.clear();
            let handle = alloc.alloc(b"test").unwrap();
            assert_eq!(alloc.get(handle), Some(&b"test"[..]));
            assert!(alloc.free(handle));
        });
    }
}
