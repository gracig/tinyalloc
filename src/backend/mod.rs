//! Allocator backend implementations
//!
//! This module contains concrete allocator implementations.

/// Slab allocator with fixed-size blocks
#[cfg(feature = "tinyslab")]
pub mod tinyslab;
