//! Utility data structures backed by allocators
//!
//! Provides high-level data structures like ByteBuffer.

#[cfg(feature = "bytebuffer")]
pub mod bytebuffer;

#[cfg(feature = "nominput")]
pub mod nominput;
