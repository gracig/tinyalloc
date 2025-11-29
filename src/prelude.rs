//! Prelude module for convenient imports
//!
//! Import everything you need with:
//! ```
//! use rsalloc::prelude::*;
//! ```

pub use crate::Allocator;
pub use crate::Handle;

#[cfg(feature = "tinyslab")]
pub use crate::backend::tinyslab::TinySlabAllocator;

#[cfg(feature = "bytebuffer")]
pub use crate::utils::bytebuffer::ByteBuffer;
