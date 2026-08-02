//! 32-byte digest used inside the vendored Kaspa kHeavyHash pipeline.

#[derive(Eq, Clone, Copy, Default, PartialOrd, Ord, Debug, PartialEq)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    #[inline(always)]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Hash(bytes)
    }

    #[inline(always)]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    #[inline(always)]
    pub fn to_le_u64(self) -> [u64; 4] {
        let mut out = [0u64; 4];
        out.iter_mut()
            .zip(self.iter_le_u64())
            .for_each(|(out, word)| *out = word);
        out
    }

    #[inline(always)]
    pub fn iter_le_u64(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.0
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
    }

    #[inline(always)]
    pub fn from_le_u64(arr: [u64; 4]) -> Self {
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_exact_mut(8).zip(arr.iter()) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        Self(out)
    }
}

impl From<[u8; 32]> for Hash {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}
