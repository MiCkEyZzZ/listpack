//! Listpack: a compact list of binary strings serialization format.
//!
//! This module implements the specification found at:
//! <https://github.com/MiCkEyZzZ/listpack>
//!
//! Internally, it stores a sequence of byte strings in a single contiguous
//! buffer using variable-length integer (varint) encoding for lengths and
//! a special terminator byte.

/// Terminator byte indicating the end of the list data.
const LP_EOF: u8 = 0xFF;

/// Mask for the lower 7 bits of a varint byte (payload).
const VARINT_VALUE_MASK: u8 = 0x7F;

/// Continuation flag in the highest bit of a varint byte.
const VARINT_CONT_MASK: u8 = 0x80;

/// Maximum value that fits in a single varint byte without continuation.
const VARINT_VALUE_MAX: usize = VARINT_VALUE_MASK as usize;

/// Threshold at which a varint must use an additional byte.
const VARINT_CONT_THRESHOLD: usize = VARINT_VALUE_MAX + 1;

/// A memory-efficient list of byte strings using custom varint-based serialization.
///
/// The underlying storage is a `Vec<u8>` centered around a terminator byte.
/// Each entry is stored as a varint-encoded length followed by the raw bytes.
pub struct Listpack {
    data: Vec<u8>,
    head: usize,
    tail: usize,
    num_entries: usize,
}

impl Listpack {
    /// Creates a new empty `Listpack` with a preallocated buffer.
    ///
    /// The internal buffer of size 1024 is centered with the terminator byte
    /// placed at the midpoint.
    pub fn new() -> Self {
        let cap = 1024;
        let mut data = vec![0; cap];
        let head = cap / 2;
        data[head] = LP_EOF;
        Self {
            data,
            head,
            tail: head + 1,
            num_entries: 0,
        }
    }

    /// Inserts a value at the front of the list.
    ///
    /// Returns `1` on success.
    ///
    /// # Arguments
    ///
    /// * `value` - A byte slice to insert at the front.
    #[inline(always)]
    pub fn push_front(&mut self, value: &[u8]) -> usize {
        let mut len_buf = [0u8; 10];
        let mut i = 0;
        let mut v = value.len();

        while v >= VARINT_CONT_THRESHOLD {
            len_buf[i] = (v as u8 & VARINT_VALUE_MASK) | VARINT_CONT_MASK;
            v >>= 7;
            i += 1;
        }

        len_buf[i] = (v as u8) & VARINT_VALUE_MASK;
        i += 1;

        let len_bytes = &len_buf[..i];
        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        // Move head backward and write len + value
        self.head -= extra;
        let h = self.head;
        self.data[h..h + len_bytes.len()].copy_from_slice(len_bytes);
        self.data[h + len_bytes.len()..h + extra].copy_from_slice(value);

        self.num_entries += 1;
        1
    }

    /// Inserts a value at the back of the list.
    ///
    /// Returns `1` on success.
    ///
    /// # Arguments
    ///
    /// * `value` - A byte slice to append.
    #[inline(always)]
    pub fn push_back(&mut self, value: &[u8]) -> usize {
        let mut len_buf = [0u8; 10];
        let mut i = 0;
        let mut v = value.len();

        while v >= VARINT_CONT_THRESHOLD {
            len_buf[i] = (v as u8 & VARINT_VALUE_MASK) | VARINT_CONT_MASK;
            v >>= 7;
            i += 1;
        }

        len_buf[i] = (v as u8) & VARINT_VALUE_MASK;
        i += 1;

        let len_bytes = &len_buf[..i];
        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        // Overwrite terminator, write length + value, then reinsert terminator
        let term_pos = self.tail - 1; // previous terminator position
        self.data[term_pos..term_pos + len_bytes.len()].copy_from_slice(len_bytes);
        let vstart = term_pos + len_bytes.len();
        self.data[vstart..vstart + value.len()].copy_from_slice(value);

        let new_term = vstart + value.len();
        self.data[new_term] = LP_EOF;
        self.tail = new_term + 1;
        self.num_entries += 1;

        1
    }

    /// Returns the number of entries in the list.
    pub fn len(&self) -> usize {
        self.num_entries
    }

    /// Returns `true` if the list contains no entries.
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    /// Retrieves a reference to the element at the specified index, if present.
    ///
    /// # Arguments
    ///
    /// * `index` - Zero-based position of the element.
    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        if index >= self.num_entries {
            return None;
        }

        let mut pos = self.head;
        let mut curr = 0;

        while pos < self.tail {
            if self.data[pos] == LP_EOF {
                break;
            }
            // decode varint from current position
            let (len, consumed) = Self::decode_varint(&self.data[pos..])?;
            if curr == index {
                return Some(&self.data[pos + consumed..pos + consumed + len]);
            }
            pos += consumed + len;
            curr += 1;
        }

        None
    }

    /// Returns an iterator over all elements in the list, from front to back.
    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        let data = &self.data;
        let mut pos = self.head;
        let end = self.tail;

        std::iter::from_fn(move || {
            if pos >= end || data[pos] == LP_EOF {
                return None;
            }

            let (len, consumed) = Self::decode_varint(&data[pos..])?;
            let start = pos + consumed;
            let slice = &data[start..start + len];
            pos = start + len;
            Some(slice)
        })
    }

    /// Removes the element at the specified index.
    ///
    /// Returns `1` if removal was successful, or `0` if index was out of bounds.
    ///
    /// # Arguments
    ///
    /// * `index` - Zero-based position to remove.
    #[inline(always)]
    pub fn remove(&mut self, index: usize) -> usize {
        if index >= self.num_entries {
            return 0;
        }

        let mut i = self.head;
        let mut curr = 0;
        while i < self.tail {
            if self.data[i] == LP_EOF {
                break;
            }

            if let Some((len, consumed)) = Self::decode_varint(&self.data[i..]) {
                if curr == index {
                    let start = i;
                    let end = i + consumed + len;
                    self.data.copy_within(end..self.tail, start);
                    self.tail -= end - start;
                    if self.tail > 0 {
                        self.data[self.tail - 1] = LP_EOF;
                    }
                    self.num_entries -= 1;
                    return 1;
                }

                i += consumed + len;
                curr += 1;
            } else {
                break;
            }
        }
        0
    }

    /// Encodes a usize value as a varint (variable-length integer).
    ///
    /// Returns a `Vec<u8>` containing the varint bytes.
    #[inline(always)]
    pub fn encode_variant(mut value: usize) -> Vec<u8> {
        let mut buf = Vec::new();

        loop {
            let byte = (value & VARINT_VALUE_MAX) as u8;
            value >>= 7;

            if value == 0 {
                buf.push(byte);
                break;
            } else {
                buf.push(byte | VARINT_CONT_MASK);
            }
        }

        buf
    }

    /// Decodes a varint from the provided byte slice.
    ///
    /// Returns `Some((value, bytes_read))` or `None` if decoding fails.
    /// consumed.
    #[inline(always)]
    pub fn decode_varint(data: &[u8]) -> Option<(usize, usize)> {
        let mut result = 0usize;
        let mut shift = 0;
        for (i, &byte) in data.iter().enumerate() {
            result |= ((byte & VARINT_VALUE_MASK) as usize) << shift;
            if byte & VARINT_CONT_MASK == 0 {
                return Some((result, i + 1));
            }

            shift += 7;

            if shift > std::mem::size_of::<usize>() * 8 {
                return None;
            }
        }

        None
    }

    /// Ensures there is enough space to insert `extra` bytes by growing
    /// and re-centering the internal buffer if necessary.
    /// bytes.
    #[inline(always)]
    fn grow_and_center(&mut self, extra: usize) {
        let used = self.tail - self.head;
        let need = used + extra + 1;

        if self.head >= extra && self.data.len() - self.tail > extra {
            return;
        }

        let new_cap = (self.len().max(1) * 3 / 2).max(need * 2);
        let mut new_data = vec![0; new_cap];

        let new_head = (new_cap - used) / 2;
        new_data[new_head..new_head + used].copy_from_slice(&self.data[self.head..self.tail]);
        self.head = new_head;
        self.tail = new_head + used;
        self.data = new_data;
    }
}

impl Default for Listpack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a newly created Listpack is empty.
    #[test]
    fn test_new_is_empty() {
        let lp = Listpack::new();

        assert!(lp.is_empty());
        assert_eq!(lp.len(), 0);
    }

    /// Tests push_back and get for correct ordering and retrieval.
    #[test]
    fn test_push_back_and_get() {
        let mut lp = Listpack::new();
        lp.push_back(b"foo");
        lp.push_back(b"bar");

        assert!(!lp.is_empty());
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(&b"foo"[..]));
        assert_eq!(lp.get(1), Some(&b"bar"[..]));
        assert_eq!(lp.get(2), None);
    }

    /// Tests push_front to ensure elements are prepended correctly.
    #[test]
    fn test_push_front_and_order() {
        let mut lp = Listpack::new();
        lp.push_front(b"foo");
        lp.push_front(b"bar");

        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(&b"bar"[..]));
        assert_eq!(lp.get(1), Some(&b"foo"[..]));
    }

    /// Tests iterator over multiple elements in correct order.
    #[test]
    fn test_iterates_correctly() {
        let mut lp = Listpack::new();
        for &v in &[&b"x"[..], &b"y"[..], &b"z"[..]] {
            lp.push_back(v);
        }
        let collected: Vec<_> = lp.iter().collect();
        assert_eq!(collected, vec![&b"x"[..], &b"y"[..], &b"z"[..]]);
    }

    /// Tests removal of a middle element and integrity of remaining data.
    #[test]
    fn test_remove_middle() {
        let mut lp = Listpack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.push_back(b"c");

        assert_eq!(lp.remove(1), 1);
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(&b"a"[..]));
        assert_eq!(lp.get(1), Some(&b"c"[..]));
    }

    /// Tests removal of the first element updates head correctly.
    #[test]
    fn test_remove_first() {
        let mut lp = Listpack::new();
        lp.push_back(b"x");
        lp.push_back(b"y");

        assert_eq!(lp.remove(0), 1);
        assert_eq!(lp.get(0), Some(&b"y"[..]));
    }

    /// Ensures remove returns 0 when index is out of bounds.
    #[test]
    fn test_remove_out_of_bounds() {
        let mut lp = Listpack::new();
        lp.push_back(b"a");

        assert_eq!(lp.remove(5), 0);
        assert_eq!(lp.len(), 1);
    }

    /// Verifies varint encoding and decoding for various values.
    #[test]
    fn test_varint_encoding_decoding() {
        let values = [0, 1, 127, 128, 255, 16384, usize::MAX >> 1];
        for &v in &values {
            let encoded = Listpack::encode_variant(v);
            let (decoded, consumed) = Listpack::decode_varint(&encoded).unwrap();

            assert_eq!(decoded, v);
            assert_eq!(consumed, encoded.len());
        }
    }

    /// Tests large number of push_back operations and iteration.
    #[test]
    fn test_large_push_and_iter() {
        let mut lp = Listpack::new();
        for i in 0..1000 {
            lp.push_back(format!("val{i}").as_bytes());
        }

        assert_eq!(lp.len(), 1000);
        assert_eq!(lp.get(0), Some(&b"val0"[..]));
        assert_eq!(lp.get(999), Some(&format!("val999").as_bytes()[..]));

        let values: Vec<_> = lp.iter().take(3).collect();

        assert_eq!(values, vec![b"val0", b"val1", b"val2"]);
    }

    /// Tests behavior on empty list for get and remove.
    #[test]
    fn test_empty_get_and_remove() {
        let lp = Listpack::new();
        assert_eq!(lp.get(0), None);

        let mut lp2 = Listpack::new();
        assert_eq!(lp2.remove(0), 0);
    }

    /// Tests zero-length entries (empty byte slices).
    #[test]
    fn test_zero_length_entries() {
        let mut lp = Listpack::new();
        lp.push_back(b"");
        lp.push_front(b"");

        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(&b""[..]));
        assert_eq!(lp.get(1), Some(&b""[..]));
    }

    /// Tests boundary lengths for varint encoding (1-, 2- and 3-byte varints).
    #[test]
    fn test_varint_boundary_lengths() {
        let mut lp = Listpack::new();
        let lengths = [
            VARINT_VALUE_MAX,
            VARINT_CONT_THRESHOLD,
            VARINT_CONT_THRESHOLD * 2 + 5,
        ];
        for &len in &lengths {
            let data = vec![b'a'; len];
            lp.push_back(&data);

            assert_eq!(lp.get(lp.len() - 1).unwrap(), data.as_slice());
        }
    }

    /// Tests multiple buffer grows with many push_back calls.
    #[test]
    fn test_buffer_grow_multiple() {
        let mut lp = Listpack::new();
        for i in 0..2000 {
            lp.push_back(format!("v{}", i).as_bytes());
        }

        assert_eq!(lp.len(), 2000);
        assert_eq!(lp.get(1000).unwrap(), format!("v{}", 1000).as_bytes());
    }

    /// Tests a single very large entry (1 million bytes).
    #[test]
    fn test_large_single_entry() {
        let mut lp = Listpack::new();
        let big = vec![0u8; 1_000_000];

        lp.push_back(&big);

        assert_eq!(lp.len(), 1);
        assert_eq!(lp.get(0).unwrap().len(), big.len());
    }

    /// Tests the Default trait implementation.
    #[test]
    fn test_default_trait() {
        let lp: Listpack = Default::default();

        assert!(lp.is_empty());
    }

    /// Tests removing the last element and reinserting.
    #[test]
    fn test_remove_last_and_reinsert() {
        let mut lp = Listpack::new();

        lp.push_back(b"end");

        assert_eq!(lp.remove(0), 1);
        assert!(lp.is_empty());

        lp.push_front(b"new");

        assert_eq!(lp.get(0), Some(&b"new"[..]));
    }

    /// Test asymmetric buffer growth: many push_front in a row
    #[test]
    fn test_asymetric_push_front_growth() {
        let mut lp = Listpack::new();
        // Insert 10_000 elements only in front
        for i in 0..10_000 {
            let s = format!("item{}", i);
            lp.push_front(s.as_bytes());
        }

        assert_eq!(lp.len(), 10_000);
        // The last inserted (i = 9_999) is now at position 0
        assert_eq!(lp.get(0), Some(format!("item{}", 9_999).as_bytes()));
        // And the first (i = 0) is at position 9_999
        assert_eq!(lp.get(9_999), Some(format!("item{}", 0).as_bytes()));
    }

    /// Test decoding of incomplete varint sequences should return None
    #[test]
    fn test_decode_incomplete_varint() {
        // 0x80 means: continuation bit = 1, but there is no next byte.
        assert!(Listpack::decode_varint(&[0x80]).is_none());
        // Two bytes, but the second one also has a continuation
        assert!(Listpack::decode_varint(&[0x81, 0x80]).is_none());
    }

    /// Test sequence integrity after multiple growth-and-center operations
    #[test]
    fn test_sequence_after_multiple_growths() {
        let mut lp = Listpack::new();
        // Alternate push_back and push_front to constantly touch both ends
        for i in 0..5000 {
            let fb = format!("B{}", i);
            lp.push_back(fb.as_bytes());
            let ff = format!("F{}", i);
            lp.push_front(ff.as_bytes());
        }

        assert_eq!(lp.len(), 10_000);
        // Check a couple of random positions.
        assert_eq!(lp.get(0), Some(format!("F4999").as_bytes()));
        assert_eq!(lp.get(1), Some(format!("F4998").as_bytes()));
        assert_eq!(lp.get(5000), Some(format!("B0").as_bytes()));
        assert_eq!(lp.get(9_999), Some(format!("B4999").as_bytes()));
    }
}
