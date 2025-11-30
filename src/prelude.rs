//! Prelude module for convenient imports
//!
//! This is the **only public API** for TinyAlloc. All items should be accessed through:
//! ```
//! use tinyalloc::prelude::*;
//! ```

pub use crate::Allocator;
pub use crate::BitLayout;
pub use crate::Handle;

#[cfg(feature = "tinyslab")]
pub use crate::backend::tinyslab::TinySlabAllocator;

#[cfg(feature = "bytebuffer")]
pub use crate::utils::bytebuffer::{ByteBuffer, ByteBufferError};

#[cfg(feature = "bytebuffer-deque")]
pub use crate::utils::bytebufferdeque::ByteBufferDeque;

#[cfg(all(feature = "bytebuffer-deque", feature = "global-alloc"))]
pub use crate::utils::bytebufferdeque::GlobalDeque;

#[cfg(feature = "global-alloc")]
pub use crate::global::{
    AllocatorStats, GlobalAllocatorConfig, init_global_allocator, stats, with_global_allocator,
};
