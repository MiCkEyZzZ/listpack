//! Listpack: a compact list of binary strings serialization
//! format.
//!
//! This module implements the specification found at:
//! <https://github.com/MiCkEyZzZ>
//!
//! Internally, it stores a sequence of byte strings in a single
//! contiguous buffer using variable-length integer (varint)
//! encoding for lengths and a special terminator byte.

/// Integer encoding tags (first byte indicates width).
const LP_ENCODING_INT8: u8 = 0x01;
const LP_ENCODING_INT16: u8 = 0x02;
const LP_ENCODING_INT24: u8 = 0x03;
const LP_ENCODING_INT32: u8 = 0x04;
const LP_ENCODING_INT64: u8 = 0x05;

/// Terminator byte indicating the end of the list data.
const LP_EOF: u8 = 0xFF;
/// Mask for the lower 7 bits of a varint byte (payload).
const VARINT_VALUE_MASK: u8 = 0x7F;
/// Continuation flag in the highest bit of a varint byte.
const VARINT_CONT_MASK: u8 = 0x80;
/// Maximum value that fits in a single varint byte without
/// continuation.
const VARINT_VALUE_MAX: usize = VARINT_VALUE_MASK as usize;
/// Threshold at which a varint must use an additional byte.
const VARINT_CONT_THRESHOLD: usize = VARINT_VALUE_MAX + 1;

/// A memory-efficient list of byte strings using varint-based serialization.
///
/// # Implementation Details
///
/// The underlying storage uses a single contiguous Vec<u8> buffer with:
/// - A terminator byte (0xFF) to mark the end of data
/// - Variable-length integer encoding for element lengths
/// - Dynamic buffer growth and recentering
pub struct Listpack {
    data: Vec<u8>,
    head: usize,
    tail: usize,
    num_entries: usize,
}

/// Iterator over Listpack elements
///
/// Provides forward iteration over the elements in the listpack.
/// Implements DoubleEndedIterator for reverse iteration.
pub struct ListpackIter<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl Listpack {
    /// Creates a new empty Listpack with default initial capacity.
    ///
    /// The internal buffer is initialized with a centered terminator byte.
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

    /// Inserts an element at the front of the list.
    ///
    /// # Arguments
    ///
    /// * value - The byte slice to insert
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if the insertion was successful, or an error if
    /// the operation failed (e.g., due to capacity constraints).
    #[inline(always)]
    pub fn push_front(&mut self, value: &[u8]) -> bool {
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

        true
    }

    /// Inserts a value at the back of the list.
    ///
    /// Returns true on success.
    ///
    /// # Arguments
    ///
    /// * value - A byte slice to append.
    #[inline(always)]
    pub fn push_back(&mut self, value: &[u8]) -> bool {
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

        true
    }

    /// Push an integer, choosing the smallest encoding automatically.
    pub fn push_integer(&mut self, value: i64) -> bool {
        let encoded = match value {
            // Для 8-битных чисел
            v if v >= i8::MIN as i64 && v <= i8::MAX as i64 => {
                let mut buf = vec![LP_ENCODING_INT8];
                buf.push(v as u8);
                buf
            }
            // Для 16-битных чисел
            v if v >= i16::MIN as i64 && v <= i16::MAX as i64 => {
                let mut buf = vec![LP_ENCODING_INT16];
                buf.extend_from_slice(&(v as i16).to_le_bytes());
                buf
            }
            // Для 24-битных чисел
            v if v >= -(1 << 23) && v <= (1 << 23) - 1 => {
                let mut buf = vec![LP_ENCODING_INT24];
                let bytes = v.to_le_bytes();
                buf.extend_from_slice(&bytes[0..3]);
                buf
            }
            // Для 32-битных чисел
            v if v >= i32::MIN as i64 && v <= i32::MAX as i64 => {
                let mut buf = vec![LP_ENCODING_INT32];
                buf.extend_from_slice(&(v as i32).to_le_bytes());
                buf
            }
            // Для 64-битных чисел
            _ => {
                let mut buf = vec![LP_ENCODING_INT64];
                buf.extend_from_slice(&value.to_le_bytes());
                buf
            }
        };

        self.push_back(&encoded)
    }

    /// Decode an integer entry from its encoded bytes.
    pub fn decode_integer(&self, data: &[u8]) -> Option<i64> {
        if data.is_empty() {
            return None;
        }

        match data[0] {
            LP_ENCODING_INT8 => {
                if data.len() < 2 {
                    return None;
                }
                Some(data[1] as i8 as i64)
            }
            LP_ENCODING_INT16 => {
                if data.len() < 3 {
                    return None;
                }
                let mut bytes = [0u8; 2];
                bytes.copy_from_slice(&data[1..3]);
                Some(i16::from_le_bytes(bytes) as i64)
            }
            LP_ENCODING_INT24 => {
                if data.len() < 4 {
                    return None;
                }
                let mut bytes = [0u8; 4];
                bytes[0..3].copy_from_slice(&data[1..4]);
                // Правильная обработка знака для 24-битного числа
                if bytes[2] & 0x80 != 0 {
                    bytes[3] = 0xFF;
                }
                Some(i32::from_le_bytes(bytes) as i64)
            }
            LP_ENCODING_INT32 => {
                if data.len() < 5 {
                    return None;
                }
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(&data[1..5]);
                Some(i32::from_le_bytes(bytes) as i64)
            }
            LP_ENCODING_INT64 => {
                if data.len() < 9 {
                    return None;
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&data[1..9]);
                Some(i64::from_le_bytes(bytes))
            }
            _ => None,
        }
    }

    /// Remove and returns the first element, or `None` if empty.
    #[inline(always)]
    pub fn pop_front(&mut self) -> Option<Vec<u8>> {
        if self.num_entries == 0 {
            return None;
        }

        let (len, consumed) = Self::decode_varint(&self.data[self.head..])?;
        let start = self.head + consumed;
        let slice = self.data[start..start + len].to_vec();
        let total = consumed + len;
        let new_head = self.head + total;
        self.head = new_head;
        self.num_entries -= 1;

        Some(slice)
    }

    /// Removes and returns the last element, or `None` if empty.
    #[inline(always)]
    pub fn pop_back(&mut self) -> Option<Vec<u8>> {
        if self.num_entries == 0 {
            return None;
        }

        let mut pos = self.head;
        let mut last_pos = self.head;
        let mut last_header = 0;

        while pos < self.tail {
            if self.data[pos] == LP_EOF {
                break;
            }

            last_pos = pos;

            if let Some((len, header)) = Self::decode_varint(&self.data[pos..]) {
                last_header = header;
                pos += header + len;
            } else {
                return None;
            }
        }

        let (len, _) = Self::decode_varint(&self.data[last_pos..]).unwrap();
        let start = last_pos + last_header;
        let slice = self.data[start..start + len].to_vec();
        let end = start + len;

        self.data.copy_within(end..self.tail, last_pos);
        self.tail -= end - last_pos;
        if self.tail > 0 {
            self.data[self.tail - 1] = LP_EOF;
        }
        self.num_entries -= 1;

        Some(slice)
    }

    /// Returns the number of entries in the list.
    pub fn len(&self) -> usize {
        self.num_entries
    }

    /// Returns `true` if the list contains no entries.
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    /// Clears all entries, resetting to initial state.
    #[inline(always)]
    pub fn clear(&mut self) {
        let cap = self.data.len();
        self.head = cap / 2;
        self.tail = self.head + 1;
        self.data[self.head] = LP_EOF;
        self.num_entries = 0;
    }

    /// Returns a reference to the first element, or `None` if empty.
    #[must_use]
    pub fn front(&self) -> Option<&[u8]> {
        self.get(0)
    }

    /// Returns a reference to the last element, or `None` if empty.
    #[must_use]
    pub fn back(&self) -> Option<&[u8]> {
        if self.num_entries == 0 {
            None
        } else {
            self.get(self.num_entries - 1)
        }
    }

    /// Retrieves a reference to the element at the specified index,
    /// if present.
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

        while pos < self.tail && self.data[pos] != LP_EOF {
            let (len, consumed) = Self::decode_varint(&self.data[pos..])?;

            if curr == index {
                return Some(&self.data[pos + consumed..pos + consumed + len]);
            }

            pos += consumed + len;
            curr += 1;
        }

        None
    }

    /// Returns a `ListpackIter` for efficient forward iteration.
    #[inline(always)]
    pub fn iter(&self) -> ListpackIter<'_> {
        ListpackIter {
            data: &self.data,
            pos: self.head,
            end: self.tail,
        }
    }

    /// Removes the element at the specified index.
    ///
    /// Returns `true` if removal was successful, or `false` if index was out
    /// of bounds.
    ///
    /// # Arguments
    ///
    /// * `index` - Zero-based position to remove.
    #[inline(always)]
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.num_entries {
            return false;
        }

        let mut i = self.head;
        let mut curr = 0;

        while i < self.tail && self.data[i] != LP_EOF {
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

                    return true;
                }
                i += consumed + len;
                curr += 1;
            } else {
                break;
            }
        }

        false
    }

    /// Encodes a usize value as a varint (variable-length integer).
    ///
    /// Returns a `Vec<u8>` containing the varint bytes.
    #[inline(always)]
    pub fn encode_varint(mut value: usize) -> Vec<u8> {
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

        // Увеличиваем размер только если действительно необходимо
        if self.head >= extra && self.data.len() - self.tail > extra {
            return;
        }

        // Более агрессивный рост для больших списков
        let growth_factor = if self.len() > 1000 { 2 } else { 3 };
        let new_cap = (self.len().max(1) * growth_factor).max(need * 2);

        // Предварительное выделение с ёмкостью, чтобы избежать лишних перекопирований
        let mut new_data = Vec::with_capacity(new_cap);
        new_data.resize(new_cap, 0);

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

impl<'a> Iterator for ListpackIter<'a> {
    type Item = &'a [u8];

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end || self.data[self.pos] == LP_EOF {
            return None;
        }

        let (len, consumed) = Listpack::decode_varint(&self.data[self.pos..])?;
        let start = self.pos + consumed;
        let slice = &self.data[start..start + len];
        self.pos = start + len;
        Some(slice)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.end - self.pos, Some(self.end - self.pos))
    }
}

impl<'a> ExactSizeIterator for ListpackIter<'a> {}

impl<'a> DoubleEndedIterator for ListpackIter<'a> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }

        let mut i = self.end - 2;

        while i > 0 && (self.data[i] & VARINT_CONT_MASK) != 0 {
            i -= 1;
        }

        let (len, consumed) = match Listpack::decode_varint(&self.data[i..self.end]) {
            Some(x) => x,
            None => return None,
        };

        let start = i + consumed;
        let slice = &self.data[start..start + len];
        self.end = i;

        Some(slice)
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

        assert_eq!(lp.remove(1), true);
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

        assert_eq!(lp.remove(0), true);
        assert_eq!(lp.get(0), Some(&b"y"[..]));
    }

    /// Ensures remove returns 0 when index is out of bounds.
    #[test]
    fn test_remove_out_of_bounds() {
        let mut lp = Listpack::new();
        lp.push_back(b"a");

        assert_eq!(lp.remove(5), false);
        assert_eq!(lp.len(), 1);
    }

    /// Verifies varint encoding and decoding for various values.
    #[test]
    fn test_varint_encoding_decoding() {
        let values = [0, 1, 127, 128, 255, 16384, usize::MAX >> 1];
        for &v in &values {
            let encoded = Listpack::encode_varint(v);
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
        assert_eq!(lp2.remove(0), false);
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

        assert_eq!(lp.remove(0), true);
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

    /// Tests pop operations from both ends of the list
    #[test]
    fn test_pop_front_and_pop_back() {
        let mut lp = Listpack::new();

        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.push_back(b"c");

        assert_eq!(lp.pop_front(), Some(b"a".to_vec()));
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.pop_back(), Some(b"c".to_vec()));
        assert_eq!(lp.len(), 1);
        assert_eq!(lp.pop_front(), Some(b"b".to_vec()));
        assert!(lp.is_empty());
        assert_eq!(lp.pop_back(), None);
    }

    #[test]
    fn test_push_and_decode_integer() {
        let mut lp = Listpack::new();

        // Тестовые значения для разных размеров
        let values = [
            0i64,           // 8 бит
            1i64,           // 8 бит
            -1i64,          // 8 бит
            127i64,         // 8 бит
            -128i64,        // 8 бит
            32767i64,       // 16 бит
            -32768i64,      // 16 бит
            8388607i64,     // 24 бит
            -8388608i64,    // 24 бит
            2147483647i64,  // 32 бит
            -2147483648i64, // 32 бит
            i64::MAX,       // 64 бит
            i64::MIN,       // 64 бит
        ];

        // Сначала добавляем все значения
        for &v in &values {
            assert!(lp.push_integer(v), "failed to push {}", v);
        }

        // Затем проверяем их
        for (i, &expected) in values.iter().enumerate() {
            let data = lp.get(i).unwrap();
            let decoded = lp.decode_integer(data).unwrap();
            assert_eq!(decoded, expected, "failed at idx {}", i);
        }
    }

    #[test]
    fn test_integer_edge_cases() {
        let mut lp = Listpack::new();

        // Граничные значения для каждого типа кодирования
        let edge_cases = [
            i8::MIN as i64,
            i8::MAX as i64,
            i16::MIN as i64,
            i16::MAX as i64,
            -(1 << 23),
            (1 << 23) - 1,
            i32::MIN as i64,
            i32::MAX as i64,
            i64::MIN,
            i64::MAX,
        ];

        // Сначала добавляем все значения
        for &v in &edge_cases {
            assert!(lp.push_integer(v), "failed to push {}", v);
        }

        // Затем проверяем их
        for (i, &expected) in edge_cases.iter().enumerate() {
            let data = lp.get(i).unwrap();
            let decoded = lp.decode_integer(data).unwrap();
            assert_eq!(decoded, expected, "failed for value {}", expected);
        }
    }

    #[test]
    fn test_mixed_push_and_pop_integer_and_string() {
        let mut lp = Listpack::new();

        // Добавляем элементы
        assert!(lp.push_integer(42));
        assert!(lp.push_back(b"hello"));
        assert!(lp.push_integer(-123));
        assert!(lp.push_back(b"world"));

        // Проверяем значения
        assert_eq!(lp.decode_integer(lp.get(0).unwrap()).unwrap(), 42);
        assert_eq!(lp.get(1).unwrap(), b"hello");
        assert_eq!(lp.decode_integer(lp.get(2).unwrap()).unwrap(), -123);
        assert_eq!(lp.get(3).unwrap(), b"world");

        // Проверяем pop операции
        let last = lp.pop_back().unwrap();
        assert_eq!(last, b"world");

        let third = lp.pop_back().unwrap();
        assert_eq!(lp.decode_integer(&third).unwrap(), -123);

        let second = lp.pop_back().unwrap();
        assert_eq!(second, b"hello");

        let first = lp.pop_back().unwrap();
        assert_eq!(lp.decode_integer(&first).unwrap(), 42);
    }
}
