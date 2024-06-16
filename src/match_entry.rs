use crate::MatchDistance;
use roaring::RoaringBitmap;
use std::fmt::{self, Debug, Formatter};

/// An entry matched during a query.
pub(crate) struct MatchEntry<'a> {
    pub distance: MatchDistance,
    pub docs: &'a RoaringBitmap,
    pub word: &'a str,
}

impl<'a> Debug for MatchEntry<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MatchEntry")
            .field("distance", &self.distance)
            .field("word", &self.word)
            .finish()
    }
}

impl<'a> PartialEq<(MatchDistance, &str)> for MatchEntry<'a> {
    fn eq(&self, other: &(MatchDistance, &str)) -> bool {
        self.distance == other.0 && self.word == other.1
    }
}
