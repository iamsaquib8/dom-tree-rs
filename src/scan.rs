//! Byte scanning: the inner loops of the tokenizer (text up to the next `<`,
//! quoted attribute values up to the closing quote, comments up to `-->`, …)
//! and of entity decoding (text up to the next `&`).
//!
//! Every search goes through [`find_byte`], so the optional `simd` feature can
//! swap the portable loop for `memchr` in one place. Both implementations
//! return the index of the *first* match, so they produce identical tokens; a
//! unit test (compiled with the feature on) checks them against each other.

/// The index of the first `needle` byte in `haystack`, if any.
#[inline]
pub(crate) fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    #[cfg(feature = "simd")]
    {
        memchr::memchr(needle, haystack)
    }
    #[cfg(not(feature = "simd"))]
    {
        portable(needle, haystack)
    }
}

/// The index of the first occurrence of `needle` in `haystack`, if any:
/// [`find_byte`] for its first byte, then a check of the rest. An empty
/// `needle` matches at 0.
#[inline]
pub(crate) fn find(needle: &[u8], haystack: &[u8]) -> Option<usize> {
    let Some((&first, rest)) = needle.split_first() else {
        return Some(0);
    };
    let mut from = 0;
    loop {
        let at = from + find_byte(first, haystack.get(from..)?)?;
        if haystack
            .get(at + 1..)
            .is_some_and(|after| after.starts_with(rest))
        {
            return Some(at);
        }
        from = at + 1;
    }
}

/// The dependency-free search: "SIMD within a register", 16 bytes per step
/// as two `u64` words (the same trick as `core`'s private `memchr`), then a
/// byte loop for the tail. On a long run with no match (Apple M4) it scans
/// ~37 GB/s against ~4 GB/s for an iterator `position` loop, which the
/// compiler does not vectorize because of the early exit; `memchr`'s NEON
/// code does ~110 GB/s.
#[cfg(any(not(feature = "simd"), test))]
fn portable(needle: u8, haystack: &[u8]) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    // A word whose bytes are zero exactly where `w` holds `needle`; the
    // returned mask flags them. Borrows only ever run towards higher bytes,
    // so in little-endian order the lowest flag is the first match.
    let splat = LO * u64::from(needle);
    let flags = |w: u64| {
        let x = w ^ splat;
        x.wrapping_sub(LO) & !x & HI
    };
    // (Always `Some` on a 16-byte chunk; the fallback matches nothing.)
    let word = |b: Option<&[u8; 8]>| b.map_or(splat ^ HI, |b| u64::from_le_bytes(*b));
    let mut chunks = haystack.chunks_exact(16);
    let mut offset = 0;
    for chunk in &mut chunks {
        let (a, b) = (word(chunk.first_chunk()), word(chunk.last_chunk()));
        let (fa, fb) = (flags(a), flags(b));
        if fa | fb != 0 {
            let bit = if fa != 0 {
                fa.trailing_zeros()
            } else {
                64 + fb.trailing_zeros()
            };
            return Some(offset + bit as usize / 8);
        }
        offset += 16;
    }
    let tail = chunks.remainder();
    tail.iter().position(|&b| b == needle).map(|i| offset + i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Haystacks around the sizes where SIMD searches switch strategy (short
    /// tails, one vector, several vectors, unaligned starts).
    fn haystacks() -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for len in [
            0usize, 1, 2, 7, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 257, 1000,
        ] {
            for at in [None, Some(0), Some(len / 2), Some(len.saturating_sub(1))] {
                let mut h: Vec<u8> = (0..len).map(|i| b'a' + (i % 23) as u8).collect();
                if let Some(at) = at {
                    if let Some(b) = h.get_mut(at) {
                        *b = b'<';
                    }
                    // A second match later on: the first must win.
                    if let Some(b) = h.get_mut(at + 9) {
                        *b = b'<';
                    }
                }
                out.push(h);
            }
        }
        out
    }

    /// The obviously correct search, as the reference.
    fn naive(needle: u8, haystack: &[u8]) -> Option<usize> {
        haystack.iter().position(|&b| b == needle)
    }

    #[test]
    fn the_portable_search_finds_the_first_match() {
        for h in haystacks() {
            for start in 0..h.len().min(40) {
                let h = &h[start..];
                for needle in [b'<', b'a', b'b', b'&', 0, 0x80, 0xff] {
                    assert_eq!(portable(needle, h), naive(needle, h), "{needle} in {h:?}");
                }
            }
        }
        // Every byte value, in every lane, next to every other byte value
        // (borrow propagation between lanes is where SWAR searches go wrong).
        for needle in 0..=255u8 {
            for other in 0..=255u8 {
                for lane in 0..17 {
                    let mut h = [other; 17];
                    h[lane] = needle;
                    assert_eq!(portable(needle, &h), naive(needle, &h));
                }
            }
        }
    }

    /// With the `simd` feature, `find_byte` is `memchr`: it must agree with
    /// the default build's search everywhere, or the two builds would
    /// tokenize differently.
    #[cfg(feature = "simd")]
    #[test]
    fn memchr_and_the_portable_search_agree() {
        for h in haystacks() {
            for start in 0..h.len().min(40) {
                let h = &h[start..];
                for needle in [b'<', b'a', b'&', b'"', b'\'', 0, 0xff] {
                    assert_eq!(memchr::memchr(needle, h), portable(needle, h));
                    assert_eq!(find_byte(needle, h), portable(needle, h));
                }
            }
        }
        for needle in 0..=255u8 {
            for other in 0..=255u8 {
                let mut h = [other; 40];
                h[33] = needle;
                assert_eq!(find_byte(needle, &h), portable(needle, &h));
            }
        }
    }

    #[test]
    fn find_finds_the_first_full_match() {
        assert_eq!(find(b"-->", b"a - -- --> -->"), Some(7));
        assert_eq!(find(b"-->", b"--"), None);
        assert_eq!(find(b"-->", b"-->"), Some(0));
        assert_eq!(find(b"-->", b"--->"), Some(1));
        assert_eq!(find(b"]]>", b"]]]>"), Some(1));
        assert_eq!(find(b"", b"x"), Some(0));
        assert_eq!(find(b"x", b""), None);
    }
}
