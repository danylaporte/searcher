use crate::{index_to_query::IndexToQuery, MatchDistance};
use std::cmp::{min, Ordering};

#[derive(Debug)]
pub(super) struct MatchDistanceScore(Vec<Rec>);

impl MatchDistanceScore {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Add a distance keeping only the best matches for each query.
    fn add_word(&mut self, index: &IndexToQuery, word: *const str) {
        if let Some(entry) = index.get(word) {
            let rec = match self.0.get_mut(entry.query_index) {
                Some(rec) => rec,
                None => {
                    self.0
                        .extend((self.0.len()..index.query_len()).map(Rec::new));
                    self.0.get_mut(entry.query_index).expect("index")
                }
            };

            rec.distance = Some(match rec.distance {
                Some(d) => min(d, entry.distance),
                None => entry.distance,
            });
        }
    }

    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    /// Add a list of words and compute the match distance score.
    pub(super) fn update(&mut self, index: &IndexToQuery, words: &[*const str]) {
        self.clear();

        for word in words {
            self.add_word(index, *word);
        }

        self.0.retain(|t| t.distance.is_some());
        self.0.sort_unstable();
    }
}

impl Eq for MatchDistanceScore {}

impl Ord for MatchDistanceScore {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut l = self.0.iter();
        let mut r = other.0.iter();

        loop {
            match (l.next(), r.next()) {
                (Some(l), Some(r)) => {
                    let o = l.cmp(r);

                    if o.is_ne() {
                        return o;
                    }
                }
                (None, Some(_)) => return Ordering::Greater,
                (Some(_), None) => return Ordering::Less,
                (None, None) => return Ordering::Equal,
            }
        }
    }
}

impl PartialEq for MatchDistanceScore {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialOrd for MatchDistanceScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug)]
struct Rec {
    distance: Option<MatchDistance>,
    index: usize,
}

impl Rec {
    fn new(index: usize) -> Self {
        Self {
            distance: None,
            index,
        }
    }
}

impl Eq for Rec {}

impl Ord for Rec {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.distance, other.distance) {
            (Some(l), Some(r)) => {
                let o = l.cmp(&r);

                if o.is_eq() {
                    self.index.cmp(&other.index)
                } else {
                    o
                }
            }

            // ordering of option must be inverted here.
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }
}

impl PartialEq for Rec {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance && self.index == other.index
    }
}

impl PartialOrd for Rec {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
