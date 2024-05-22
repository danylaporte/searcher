#[derive(Clone, Copy)]
pub(crate) enum WordQueryOp {
    Contains,
    Eq,
    EndsWith,
    Fuzzy,
    StartsWith,
}
