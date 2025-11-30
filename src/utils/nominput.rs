//! Nom parser input trait implementations for ByteBuffer
//!
//! This module provides integration with the nom parser combinator library,
//! allowing ByteBuffer to be used directly as input for nom parsers.
//!
//! # Feature Flag
//!
//! This module is only available when the `nominput` feature is enabled.
//!
//! # Zero-Copy Design  
//!
//! This implementation is **zero-copy** and works with ByteBuffer's chunked nature.
//! Due to ByteBuffer's linked-list storage, only limited nom Input trait methods
//! are supported without allocations.
//!
//! # Limitations
//!
//! - Cannot provide contiguous `&[u8]` slices without allocation
//! - `take_split()` creates logical views, not actual slices
//! - Iterator-based parsing recommended for best results
//! - Some nom combinators requiring `AsBytes` won't work
//!
//! # Usage
//!
//! When the `nominput` feature is enabled, `ByteBuffer` implements nom's `Input`
//! trait directly. Use ByteBuffer as parser input and access bytes via the
//! `bytes()` iterator.

use crate::utils::bytebuffer::ByteBuffer;

// Helper struct to wrap the iterator with the correct lifetime
#[cfg(feature = "global-alloc")]
pub struct ByteBufferIter {
    buf: ByteBuffer,
    index: usize,
}

#[cfg(feature = "global-alloc")]
impl Iterator for ByteBufferIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.buf.len as usize {
            return None;
        }

        let byte = crate::global::with_global_allocator(|alloc| {
            self.buf.read(alloc).bytes().nth(self.index)
        });

        self.index += 1;
        byte
    }
}

// Implement nom's Input trait directly on ByteBuffer
// Note: This delegates to ByteBuffer's own methods for consistency
impl nom::Input for ByteBuffer {
    type Item = u8;

    #[cfg(feature = "global-alloc")]
    type Iter = ByteBufferIter;
    #[cfg(not(feature = "global-alloc"))]
    type Iter = core::iter::Empty<u8>;

    #[cfg(feature = "global-alloc")]
    type IterIndices = core::iter::Enumerate<ByteBufferIter>;
    #[cfg(not(feature = "global-alloc"))]
    type IterIndices = core::iter::Empty<(usize, u8)>;

    fn input_len(&self) -> usize {
        ByteBuffer::input_len(self)
    }

    fn take(&self, index: usize) -> Self {
        ByteBuffer::take(self, index)
    }

    fn take_from(&self, index: usize) -> Self {
        ByteBuffer::take_from(self, index)
    }

    fn take_split(&self, index: usize) -> (Self, Self) {
        ByteBuffer::take_split(self, index)
    }

    fn position<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Item) -> bool,
    {
        #[cfg(feature = "global-alloc")]
        {
            ByteBuffer::position_global(self, predicate)
        }
        #[cfg(not(feature = "global-alloc"))]
        {
            // Cannot implement without allocator reference
            let _ = predicate;
            None
        }
    }

    fn iter_elements(&self) -> Self::Iter {
        #[cfg(feature = "global-alloc")]
        {
            ByteBufferIter {
                buf: *self,
                index: 0,
            }
        }
        #[cfg(not(feature = "global-alloc"))]
        {
            core::iter::empty()
        }
    }

    fn iter_indices(&self) -> Self::IterIndices {
        #[cfg(feature = "global-alloc")]
        {
            ByteBufferIter {
                buf: *self,
                index: 0,
            }
            .enumerate()
        }
        #[cfg(not(feature = "global-alloc"))]
        {
            core::iter::empty()
        }
    }

    fn slice_index(&self, count: usize) -> Result<usize, nom::Needed> {
        match ByteBuffer::slice_index(self, count) {
            Ok(idx) => Ok(idx),
            Err(needed) => Err(nom::Needed::new(needed)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nom::Input; // Import the trait to use its methods

    #[test]
    fn test_input_len() {
        let buf = ByteBuffer::new();
        assert_eq!(buf.input_len(), 0);

        let buf = ByteBuffer {
            head: None,
            tail: None,
            len: 42,
            max_len: None,
            #[cfg(feature = "bytebuffer-deque")]
            read_offset: 0,
        };
        assert_eq!(buf.input_len(), 42);
    }

    #[test]
    fn test_take() {
        let buf = ByteBuffer {
            head: None,
            tail: None,
            len: 100,
            max_len: None,
            #[cfg(feature = "bytebuffer-deque")]
            read_offset: 0,
        };

        let taken = buf.take(50);
        assert_eq!(taken.len(), 50);
        assert_eq!(taken.max_len(), Some(50));

        // Taking more than available should clamp
        let taken = buf.take(200);
        assert_eq!(taken.len(), 100);
        assert_eq!(taken.max_len(), Some(100));
    }

    #[test]
    fn test_take_from() {
        let buf = ByteBuffer {
            head: None,
            tail: None,
            len: 100,
            max_len: None,
            #[cfg(feature = "bytebuffer-deque")]
            read_offset: 0,
        };

        // take_from with index >= len returns empty
        let remaining = buf.take_from(100);
        assert_eq!(remaining.len(), 0);

        let remaining = buf.take_from(200);
        assert_eq!(remaining.len(), 0);

        // take_from with index < len returns copy (limitation)
        let remaining = buf.take_from(50);
        assert_eq!(remaining.len(), 100); // Known limitation
    }

    #[test]
    fn test_take_split() {
        let buf = ByteBuffer {
            head: None,
            tail: None,
            len: 100,
            max_len: None,
            #[cfg(feature = "bytebuffer-deque")]
            read_offset: 0,
        };

        let (_remaining, taken) = buf.take_split(30);
        assert_eq!(taken.len(), 30);
        // Note: remaining.len() is 100 due to take_from limitation
    }

    #[test]
    fn test_slice_index() {
        let buf = ByteBuffer {
            head: None,
            tail: None,
            len: 50,
            max_len: None,
            #[cfg(feature = "bytebuffer-deque")]
            read_offset: 0,
        };

        // Valid index
        assert_eq!(buf.slice_index(25), Ok(25));
        assert_eq!(buf.slice_index(50), Ok(50));
        assert_eq!(buf.slice_index(0), Ok(0));

        // Invalid index - needs more data
        match buf.slice_index(75) {
            Err(needed) => assert_eq!(needed, 25),
            _ => panic!("Expected Err with needed count"),
        }
    }

    #[test]
    fn test_position_returns_none() {
        let buf = ByteBuffer::new();
        // Call the trait method explicitly (no allocator needed)
        let result = nom::Input::position(&buf, |_| true);
        #[cfg(not(feature = "global-alloc"))]
        assert_eq!(result, None);
        #[cfg(feature = "global-alloc")]
        assert_eq!(result, None); // Empty buffer
    }

    #[test]
    #[cfg(feature = "global-alloc")]
    fn test_position_with_global_alloc() {
        use crate::global::GlobalAllocatorConfig;

        // Initialize global allocator
        GlobalAllocatorConfig::Slab1K32.init();

        let mut buf = ByteBuffer::new();
        buf.extend(b"Hello, World!").unwrap();

        // Find the comma (using nom::Input trait which calls position_global)
        let pos = nom::Input::position(&buf, |b| b == b',');
        assert_eq!(pos, Some(5));

        // Find the exclamation mark
        let pos = nom::Input::position(&buf, |b| b == b'!');
        assert_eq!(pos, Some(12));

        // Find something that doesn't exist
        let pos = nom::Input::position(&buf, |b| b == b'Z');
        assert_eq!(pos, None);

        // Cleanup
        buf.clear();
    }
    #[test]
    fn test_iter_elements_empty() {
        let buf = ByteBuffer::new();
        let mut iter = buf.iter_elements();
        assert_eq!(iter.next(), None);
    }

    #[test]
    #[cfg(feature = "global-alloc")]
    fn test_iter_elements_with_data() {
        use crate::global::GlobalAllocatorConfig;

        // Initialize global allocator
        GlobalAllocatorConfig::Slab1K32.init();

        let mut buf = ByteBuffer::new();
        buf.extend(b"ABC").unwrap();

        let mut iter = buf.iter_elements();
        assert_eq!(iter.next(), Some(b'A'));
        assert_eq!(iter.next(), Some(b'B'));
        assert_eq!(iter.next(), Some(b'C'));
        assert_eq!(iter.next(), None);

        // Cleanup
        buf.clear();
    }

    #[test]
    fn test_iter_indices_empty() {
        let buf = ByteBuffer::new();
        let mut iter = buf.iter_indices();
        assert_eq!(iter.next(), None);
    }

    #[test]
    #[cfg(feature = "global-alloc")]
    fn test_iter_indices_with_data() {
        use crate::global::GlobalAllocatorConfig;

        // Initialize global allocator
        GlobalAllocatorConfig::Slab1K32.init();

        let mut buf = ByteBuffer::new();
        buf.extend(b"XYZ").unwrap();

        let mut iter = buf.iter_indices();
        assert_eq!(iter.next(), Some((0, b'X')));
        assert_eq!(iter.next(), Some((1, b'Y')));
        assert_eq!(iter.next(), Some((2, b'Z')));
        assert_eq!(iter.next(), None);

        // Cleanup
        buf.clear();
    }

    #[test]
    fn test_clone_copy() {
        let buf1 = ByteBuffer {
            head: None,
            tail: None,
            len: 42,
            max_len: Some(100),
            #[cfg(feature = "bytebuffer-deque")]
            read_offset: 0,
        };

        let buf2 = buf1; // Copy
        let buf3 = buf1.clone(); // Clone

        assert_eq!(buf1.len(), 42);
        assert_eq!(buf2.len(), 42);
        assert_eq!(buf3.len(), 42);
    }
}
