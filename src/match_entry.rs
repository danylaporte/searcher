use crate::{Entry, MatchDistance};
use std::fmt::{self, Debug, Formatter};

/// An entry matched during a query.
pub(crate) struct MatchEntry<'a> {
    pub distance: MatchDistance,
    pub entry: &'a Entry,
}

impl<'a> Debug for MatchEntry<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MatchEntry")
            .field("distance", &self.distance)
            .field("word", &self.entry.word)
            .finish()
    }
}

impl<'a> PartialEq<(MatchDistance, &str)> for MatchEntry<'a> {
    fn eq(&self, other: &(MatchDistance, &str)) -> bool {
        self.distance == other.0 && &*self.entry.word == other.1
    }
}
