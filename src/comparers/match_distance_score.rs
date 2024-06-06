use crate::{index_to_query::IndexToQuery, MatchDistance};
use std::cmp::{min, Ordering};

pub(super) struct MatchDistanceScore {
    dirty: bool,
    recs: Vec<Rec>,
}

impl MatchDistanceScore {
    pub const fn new() -> Self {
        Self {
            dirty: false,
            recs: Vec::new(),
        }
    }

    /// Add a distance keeping only the best matches for each query.
    fn add_word(&mut self, index: &IndexToQuery, word: *const str) {
        if let Some(entry) = index.get(word) {
            let rec = match self.recs.get_mut(entry.query_index) {
                Some(rec) => rec,
                None => {
                    self.ensure_size(index.query_len());
                    self.recs.get_mut(entry.query_index).expect("index")
                }
            };

            rec.distance = Some(match rec.distance {
                Some(d) => min(d, entry.distance),
                None => entry.distance,
            });

            self.dirty = true;
        }
    }

    pub(super) fn clear(&mut self) {
        if self.dirty {
            self.recs
                .iter_mut()
                .enumerate()
                .for_each(|(index, r)| *r = Rec::new(index));

            self.dirty = false;
        }
    }

    fn ensure_size(&mut self, len: usize) {
        if self.recs.len() < len {
            let from = self.recs.len();
            let range = from..len;

            self.recs.extend(range.map(Rec::new));
        }
    }

    /// Add a list of words and compute the match distance score.
    pub(super) fn update(&mut self, index: &IndexToQuery, words: &[*const str]) {
        self.clear();

        for word in words {
            self.add_word(index, *word);
        }

        if self.dirty {
            self.recs.sort_unstable();
        }
    }
}

impl Eq for MatchDistanceScore {}

impl Ord for MatchDistanceScore {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut o = self.recs.is_empty().cmp(&other.recs.is_empty());

        if o.is_eq() {
            o = self.recs.cmp(&other.recs);
        }

        o
    }
}

impl PartialEq for MatchDistanceScore {
    fn eq(&self, other: &Self) -> bool {
        self.recs == other.recs
    }
}

impl PartialOrd for MatchDistanceScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy)]
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
