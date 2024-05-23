use uuid::Uuid;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct AttrId(Uuid);

impl AttrId {
    pub const fn new(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<Uuid> for AttrId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
