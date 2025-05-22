//! Listpack a lists of strings serialization format
//!
//! This file implements the specification you can
//! find at: https://github.com/MiCkEyZzZ/listpack

/// Терминатор списка — байт 0xFF.
const LP_EOF: u8 = 0xFF;
/// Маска для младших 7 бит в varint-байте (payload).
const VARINT_VALUE_MASK: u8 = 0x7F;
/// Флаг «продолжение» в старшем бите varint-байта.
const VARINT_CONT_MASK: u8 = 0x80;
/// Максимальное значение, которое помещается в один varint-байт без continuation.
const VARINT_VALUE_MAX: usize = VARINT_VALUE_MASK as usize;
/// Порог, при достижении или превышении которого нужен следующий байт varint.
const VARINT_CONT_THRESHOLD: usize = VARINT_VALUE_MAX + 1;

pub struct Listpack {
    data: Vec<u8>,
    head: usize,
    tail: usize,
    num_entries: usize,
}

impl Listpack {
    /// Creates a new empty `Listpack` with preallocated
    /// capacity and a termonator at the center.
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

    /// Insert a value at the front of the list.
    pub fn push_front(&mut self, value: &[u8]) -> usize {
        let mut len_bytes = Vec::new();
        let mut v = value.len();

        while v >= VARINT_CONT_THRESHOLD {
            len_bytes.push((v as u8 & VARINT_VALUE_MASK) | VARINT_CONT_MASK);
            v >>= 7;
        }

        len_bytes.push((v as u8) & VARINT_VALUE_MASK);

        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        // Move head backward and write len + value
        self.head -= extra;
        let h = self.head;
        self.data[h..h + len_bytes.len()].copy_from_slice(&len_bytes);
        self.data[h + len_bytes.len()..h + extra].copy_from_slice(value);

        self.num_entries += 1;
        1
    }

    /// Insert a value at the back of the list.
    pub fn push_back(&mut self, value: &[u8]) -> usize {
        let mut len_bytes = Vec::new();
        let mut v = value.len();

        while v >= VARINT_CONT_THRESHOLD {
            len_bytes.push((v as u8 & VARINT_VALUE_MASK) | VARINT_CONT_MASK);
            v >>= 7;
        }

        len_bytes.push((v as u8) & VARINT_VALUE_MASK);

        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        let term_pos = self.tail - 1;
        self.data[term_pos..term_pos + len_bytes.len()].copy_from_slice(&len_bytes);
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

    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    /// Returns a referance to the element at the specified index,
    /// if it exists.
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

    /// Returns an iterator over all elements in the lisy.
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        let data = &self.data;
        let mut pos = self.head;
        let end = self.tail;

        std::iter::from_fn(move || {
            if pos >= end || data[pos] == LP_EOF {
                return None;
            }

            let (len, consumed) = Listpack::decode_varint(&data[pos..])?;
            let start = pos + consumed;
            let slice = &data[start..start + len];
            pos = start + len;
            Some(slice)
        })
    }

    /// Removes the element at the specified index. Returns true
    /// if successful.
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

    /// Encodes a `usize` value as a variable-length integer
    /// (varint).
    pub fn encode_variant(mut value: usize) -> Vec<u8> {
        let mut buf = Vec::new();

        loop {
            let byte = (value & VARINT_VALUE_MAX) as u8; // take lowest 7 bits
            value >>= 7;

            if value == 0 {
                buf.push(byte); // last byte: continuation bit is not set
                break;
            } else {
                buf.push(byte | VARINT_CONT_MASK); // Set continuation bit (more bytes follow)
            }
        }

        buf
    }

    /// Decodes a variable-length integer (varint) from the given
    /// slice. Returns the decoded value and the number of bytes
    /// consumed.
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

    /// Amortized expansion and centering of the internal buffer
    /// if needed. Ensures there is enough space to insert `extra`
    /// bytes.
    fn grow_and_center(&mut self, extra: usize) {
        let used = self.tail - self.head;
        let need = used + extra + 1;

        if self.head >= extra && self.data.len() - self.tail > extra {
            // Already enough space.
            return;
        }

        // New capacity: max(1.5x, need).
        let new_cap = (self.len().max(1) * 3 / 2).max(need * 2);
        let mut new_data = vec![0; new_cap];

        // Center the current content in the new bugger.
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
    use crate::Listpack;

    #[test]
    fn test_new_is_empty() {
        let lp = Listpack::new();

        assert!(lp.is_empty());
        assert_eq!(lp.len(), 0);
    }

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

    #[test]
    fn test_push_front() {
        let mut lp = Listpack::new();
        lp.push_front(b"foo");
        lp.push_front(b"bar");

        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(&b"bar"[..]));
        assert_eq!(lp.get(1), Some(&b"foo"[..]));
    }

    #[test]
    fn test_iter() {
        let mut lp = Listpack::new();
        lp.push_back(b"one");
        lp.push_back(b"two");
        lp.push_back(b"three");

        let items: Vec<_> = lp.iter().collect();

        assert_eq!(items, vec![&b"one"[..], &b"two"[..], &b"three"[..]]);
    }

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

    #[test]
    fn test_remove_first() {
        let mut lp = Listpack::new();
        lp.push_back(b"x");
        lp.push_back(b"y");

        assert_eq!(lp.remove(0), 1);
        assert_eq!(lp.get(0), Some(&b"y"[..]));
    }

    #[test]
    fn test_remove_out_of_bounds() {
        let mut lp = Listpack::new();
        lp.push_back(b"a");

        assert_eq!(lp.remove(5), 0);
        assert_eq!(lp.len(), 1);
    }

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

    #[test]
    fn test_empty_get_and_remove() {
        let lp = Listpack::new();
        assert_eq!(lp.get(0), None);

        let mut lp2 = Listpack::new();
        assert_eq!(lp2.remove(0), 0);
    }
}
