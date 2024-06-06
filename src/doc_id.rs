#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct DocId(pub(crate) u32);

impl DocId {
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl From<u32> for DocId {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}
