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
use core::mem::{MaybeUninit, size_of};
use core::ptr;

/// Maximum allocator size that can be stored
const MAX_ALLOC_SIZE: usize = 8192;

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
    pub fn init(self) {
        match self {
            GlobalAllocatorConfig::Slab256b32 => {
                init_global_allocator(TinySlabAllocator::<256, 8>::new())
            }
            GlobalAllocatorConfig::Slab256b16 => {
                init_global_allocator(TinySlabAllocator::<256, 16>::new())
            }
            GlobalAllocatorConfig::Slab256b8 => {
                init_global_allocator(TinySlabAllocator::<256, 32>::new())
            }
            GlobalAllocatorConfig::Slab512b32 => {
                init_global_allocator(TinySlabAllocator::<512, 16>::new())
            }
            GlobalAllocatorConfig::Slab512b16 => {
                init_global_allocator(TinySlabAllocator::<512, 32>::new())
            }
            GlobalAllocatorConfig::Slab512b8 => {
                init_global_allocator(TinySlabAllocator::<512, 64>::new())
            }
            GlobalAllocatorConfig::Slab768b32 => {
                init_global_allocator(TinySlabAllocator::<768, 24>::new())
            }
            GlobalAllocatorConfig::Slab768b16 => {
                init_global_allocator(TinySlabAllocator::<768, 48>::new())
            }
            GlobalAllocatorConfig::Slab768b8 => {
                init_global_allocator(TinySlabAllocator::<768, 96>::new())
            }
            GlobalAllocatorConfig::Slab1K32 => {
                init_global_allocator(TinySlabAllocator::<1024, 32>::new())
            }
            GlobalAllocatorConfig::Slab1K16 => {
                init_global_allocator(TinySlabAllocator::<1024, 64>::new())
            }
            GlobalAllocatorConfig::Slab1K8 => {
                init_global_allocator(TinySlabAllocator::<1024, 128>::new())
            }
        }
    }
}

// Type aliases for complex function pointer types
type AllocFn = unsafe fn(*mut u8, &[u8]) -> Option<Handle>;
type AllocUninitFn = unsafe fn(*mut u8, usize) -> Option<(Handle, &'static mut [u8])>;
type FreeFn = unsafe fn(*mut u8, Handle) -> bool;
type GetFn = unsafe fn(*const u8, Handle) -> Option<&'static [u8]>;
type GetMutFn = unsafe fn(*mut u8, Handle) -> Option<&'static mut [u8]>;
type LenFn = unsafe fn(*const u8) -> usize;
type CapacityFn = unsafe fn(*const u8) -> usize;
type BlockSizeFn = unsafe fn(*const u8) -> usize;
type BitLayoutFn = unsafe fn(*const u8) -> crate::BitLayout;
type ClearFn = unsafe fn(*mut u8);

/// Type-erased global allocator storage
struct GlobalStorage {
    /// Raw storage buffer for any allocator up to MAX_ALLOC_SIZE
    storage: UnsafeCell<MaybeUninit<[u8; MAX_ALLOC_SIZE]>>,
    /// Function pointers for dynamic dispatch (vtable)
    /// UnsafeCell needed because init() mutates through &self (static context)
    alloc_fn: UnsafeCell<Option<AllocFn>>,
    alloc_uninit_fn: UnsafeCell<Option<AllocUninitFn>>,
    free_fn: UnsafeCell<Option<FreeFn>>,
    get_fn: UnsafeCell<Option<GetFn>>,
    get_mut_fn: UnsafeCell<Option<GetMutFn>>,
    len_fn: UnsafeCell<Option<LenFn>>,
    capacity_fn: UnsafeCell<Option<CapacityFn>>,
    block_size_fn: UnsafeCell<Option<BlockSizeFn>>,
    bit_layout_fn: UnsafeCell<Option<BitLayoutFn>>,
    clear_fn: UnsafeCell<Option<ClearFn>>,
}

unsafe impl Sync for GlobalStorage {}

impl GlobalStorage {
    const fn new() -> Self {
        Self {
            storage: UnsafeCell::new(MaybeUninit::uninit()),
            alloc_fn: UnsafeCell::new(None),
            alloc_uninit_fn: UnsafeCell::new(None),
            free_fn: UnsafeCell::new(None),
            get_fn: UnsafeCell::new(None),
            get_mut_fn: UnsafeCell::new(None),
            len_fn: UnsafeCell::new(None),
            capacity_fn: UnsafeCell::new(None),
            block_size_fn: UnsafeCell::new(None),
            bit_layout_fn: UnsafeCell::new(None),
            clear_fn: UnsafeCell::new(None),
        }
    }

    fn init<A: Allocator + 'static>(&self, allocator: A) {
        assert!(
            size_of::<A>() <= MAX_ALLOC_SIZE,
            "Allocator size {} exceeds MAX_ALLOC_SIZE {}",
            size_of::<A>(),
            MAX_ALLOC_SIZE
        );

        unsafe {
            // Write allocator into storage
            let storage_ptr = (*self.storage.get()).as_mut_ptr() as *mut A;
            ptr::write(storage_ptr, allocator);

            // Setup function pointers
            *self.alloc_fn.get() = Some(|ptr, data| {
                let alloc = &mut *(ptr as *mut A);
                Allocator::alloc(alloc, data)
            });

            *self.alloc_uninit_fn.get() = Some(|ptr, len| {
                let alloc = &mut *(ptr as *mut A);
                Allocator::alloc_uninit(alloc, len)
            });

            *self.free_fn.get() = Some(|ptr, handle| {
                let alloc = &mut *(ptr as *mut A);
                Allocator::free(alloc, handle)
            });

            *self.get_fn.get() = Some(|ptr, handle| {
                let alloc = &*(ptr as *const A);
                Allocator::get(alloc, handle)
            });

            *self.get_mut_fn.get() = Some(|ptr, handle| {
                let alloc = &mut *(ptr as *mut A);
                Allocator::get_mut(alloc, handle)
            });

            *self.len_fn.get() = Some(|ptr| {
                let alloc = &*(ptr as *const A);
                Allocator::len(alloc)
            });

            *self.capacity_fn.get() = Some(|ptr| {
                let alloc = &*(ptr as *const A);
                Allocator::capacity(alloc)
            });

            *self.block_size_fn.get() = Some(|ptr| {
                let alloc = &*(ptr as *const A);
                Allocator::block_size(alloc)
            });

            *self.bit_layout_fn.get() = Some(|ptr| {
                let alloc = &*(ptr as *const A);
                Allocator::bit_layout(alloc)
            });

            *self.clear_fn.get() = Some(|ptr| {
                let alloc = &mut *(ptr as *mut A);
                Allocator::clear(alloc)
            });
        }
    }

    #[inline]
    fn storage_ptr(&self) -> *mut u8 {
        unsafe { (*self.storage.get()).as_mut_ptr() as *mut u8 }
    }

    #[inline]
    fn storage_const_ptr(&self) -> *const u8 {
        unsafe { (*self.storage.get()).as_ptr() as *const u8 }
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
/// fn main() {
///     // Optional: 4KB allocator with 128 slots
///     init_global_allocator(TinySlabAllocator::<4096, 128>::new());
/// }
/// ```
pub fn init_global_allocator<A: Allocator + 'static>(allocator: A) {
    GLOBAL.init(allocator);
}

/// Allocate a block from the global allocator
#[inline]
pub fn alloc(data: &[u8]) -> Option<Handle> {
    unsafe {
        let func = (*GLOBAL.alloc_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_ptr(), data)
    }
}

/// Allocate an uninitialized block from the global allocator
#[inline]
pub fn alloc_uninit(len: usize) -> Option<(Handle, &'static mut [u8])> {
    unsafe {
        let func = (*GLOBAL.alloc_uninit_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_ptr(), len)
    }
}

/// Free a block in the global allocator
#[inline]
pub fn free(handle: Handle) -> bool {
    unsafe {
        let func = (*GLOBAL.free_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_ptr(), handle)
    }
}

/// Get immutable reference to a block
#[inline]
pub fn get(handle: Handle) -> Option<&'static [u8]> {
    unsafe {
        let func = (*GLOBAL.get_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_const_ptr(), handle)
    }
}

/// Get mutable reference to a block
#[inline]
pub fn get_mut(handle: Handle) -> Option<&'static mut [u8]> {
    unsafe {
        let func = (*GLOBAL.get_mut_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_ptr(), handle)
    }
}

/// Get number of allocated blocks
#[inline]
pub fn len() -> usize {
    unsafe {
        let func = (*GLOBAL.len_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_const_ptr())
    }
}

/// Get total capacity
#[inline]
pub fn capacity() -> usize {
    unsafe {
        let func = (*GLOBAL.capacity_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_const_ptr())
    }
}

/// Get block size
#[inline]
pub fn block_size() -> usize {
    unsafe {
        let func = (*GLOBAL.block_size_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_const_ptr())
    }
}

/// Get bit layout
#[inline]
pub fn bit_layout() -> crate::BitLayout {
    unsafe {
        let func = (*GLOBAL.bit_layout_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_const_ptr())
    }
}

/// Clear all allocations
#[inline]
pub fn clear() {
    unsafe {
        let func = (*GLOBAL.clear_fn.get()).unwrap_unchecked();
        func(GLOBAL.storage_ptr())
    }
}

/// Get statistics about the global allocator
pub fn stats() -> AllocatorStats {
    AllocatorStats {
        used: len(),
        capacity: capacity(),
        block_size: block_size(),
    }
}

/// Wrapper type that implements Allocator trait by forwarding to global functions
/// This allows ByteBuffer to work with the global allocator
pub struct GlobalAllocatorRef;

impl Allocator for GlobalAllocatorRef {
    fn alloc(&mut self, data: &[u8]) -> Option<Handle> {
        alloc(data)
    }

    fn alloc_uninit(&mut self, len: usize) -> Option<(Handle, &mut [u8])> {
        alloc_uninit(len)
    }

    fn free(&mut self, handle: Handle) -> bool {
        free(handle)
    }

    fn get(&self, handle: Handle) -> Option<&[u8]> {
        get(handle)
    }

    fn get_mut(&mut self, handle: Handle) -> Option<&mut [u8]> {
        get_mut(handle)
    }

    fn len(&self) -> usize {
        len()
    }

    fn capacity(&self) -> usize {
        capacity()
    }

    fn block_size(&self) -> usize {
        block_size()
    }

    fn bit_layout(&self) -> crate::BitLayout {
        bit_layout()
    }

    fn clear(&mut self) {
        clear()
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
        if (*GLOBAL.alloc_fn.get()).is_none() {
            GlobalAllocatorConfig::Slab256b8.init();
        }
    }
    f(&mut GlobalAllocatorRef)
}

/// Statistics about the global allocator
#[derive(Debug, Clone, Copy)]
pub struct AllocatorStats {
    /// Number of currently allocated slots
    pub used: usize,
    /// Total number of available slots
    pub capacity: usize,
    /// Size of each block in bytes
    pub block_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::tinyslab::TinySlabAllocator;

    #[test]
    fn test_direct_alloc() {
        // Init
        init_global_allocator(TinySlabAllocator::<2048, 64>::new());

        assert_eq!(capacity(), 64);
        assert_eq!(block_size(), 32);
        assert_eq!(len(), 0);

        // Try to allocate directly
        let result = alloc(b"test");
        assert!(result.is_some(), "Allocation should succeed");

        let handle = result.unwrap();
        assert_eq!(get(handle), Some(&b"test"[..]));
        assert_eq!(len(), 1);

        assert!(free(handle));
        assert_eq!(len(), 0);
    }

    #[test]
    #[cfg(feature = "bytebuffer")]
    fn test_bytebuffer_with_global() {
        use crate::utils::bytebuffer::ByteBuffer;

        // Init
        init_global_allocator(TinySlabAllocator::<2048, 64>::new());
        clear();

        // Create buffer and try to append
        let mut buf = ByteBuffer::new();
        let result = buf.append(42);

        assert!(result.is_ok(), "ByteBuffer append should succeed");
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn test_init_custom_size() {
        // Initialize with custom 4KB allocator
        init_global_allocator(TinySlabAllocator::<4096, 128>::new());

        // Verify configuration
        assert_eq!(capacity(), 128);
        assert_eq!(block_size(), 32);

        // Test allocation
        clear();
        let handle = alloc(b"test").unwrap();
        assert_eq!(get(handle), Some(&b"test"[..]));
        assert!(free(handle));
    }
}
