#[derive(Clone, Copy, Eq, PartialEq)]
pub enum MinMatchLevel {
    Fuzzy,
    Contains,
    Equal,
}
