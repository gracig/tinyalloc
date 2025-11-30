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

#[cfg(all(feature = "bytebuffer-deque", feature = "global-alloc"))]
pub use crate::utils::bytebuffer::GlobalDeque;

#[cfg(feature = "global-alloc")]
pub use crate::global::{GlobalAllocatorConfig, init_global_allocator, with_global_allocator};

// Nom integration traits are implemented directly on ByteBuffer when nominput feature is enabled
// No additional types need to be exported
