#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WordQueryOp {
    Contains,
    Eq,
    EndsWith,
    Fuzzy,
    StartsWith,
}
