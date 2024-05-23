use uuid::Uuid;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct AttrId(Uuid);

impl From<Uuid> for AttrId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}
