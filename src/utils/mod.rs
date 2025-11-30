//! Utility data structures backed by allocators
//!
//! Provides high-level data structures like ByteBuffer and ByteBufferDeque.

#[cfg(feature = "bytebuffer")]
pub mod bytebuffer;

#[cfg(feature = "bytebuffer-deque")]
pub mod bytebufferdeque;
