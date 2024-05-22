use std::cmp::Ordering;

/// Word match distance for the scoring the relevance of a doc in a query.
#[derive(Clone, Copy, Debug, Eq)]
pub(crate) enum MatchDistance {
    Exact(u8),
    Fuzzy(u8),
}

impl PartialEq for MatchDistance {
    fn eq(&self, other: &Self) -> bool {
        // faster manual implementation
        match (self, other) {
            (Self::Fuzzy(l0), Self::Fuzzy(r0)) => l0 == r0,
            (Self::Exact(l0), Self::Exact(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl PartialOrd for MatchDistance {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MatchDistance {
    fn cmp(&self, other: &Self) -> Ordering {
        // faster manual implementation
        match (self, other) {
            (Self::Fuzzy(a), Self::Fuzzy(b)) => a.cmp(b),
            (Self::Fuzzy(_), _) => Ordering::Greater,
            (Self::Exact(a), Self::Exact(b)) => a.cmp(b),
            (Self::Exact(_), _) => Ordering::Less,
        }
    }
}
