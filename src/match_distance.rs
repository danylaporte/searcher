/// Word match distance for the scoring the relevance of a doc in a query.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct MatchDistance(pub(crate) u8);
