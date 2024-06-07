use crate::{match_entry::MatchEntry, MatchDistance, WordQuery};
use fxhash::FxHashMap;
use roaring::RoaringBitmap;
use std::{cmp::max, collections::hash_map::Entry};

/// A reverse index to find WordQuery from indexed word.
///
/// Words that have the best
#[derive(Default)]
pub(crate) struct IndexToQuery<'a> {
    map: FxHashMap<*const str, IndexToQueryEntry<'a>>,
    query_len: usize,
}

impl<'a> IndexToQuery<'a> {
    /// Add a match entry associated with a query, keeping only the best matches.
    pub fn add(&mut self, query: &WordQuery, match_entry: MatchEntry<'a>) {
        match self.map.entry(&*match_entry.entry.word) {
            Entry::Occupied(mut o) => {
                let o = o.get_mut();

                if (o.distance, o.query_index) > (match_entry.distance, query.index) {
                    o.distance = match_entry.distance;
                    o.docs = &match_entry.entry.docs;
                    o.query_index = query.index;
                }
            }
            Entry::Vacant(v) => {
                v.insert(IndexToQueryEntry {
                    distance: match_entry.distance,
                    docs: &match_entry.entry.docs,
                    query_index: query.index,
                });
            }
        }

        self.query_len = max(self.query_len, query.index + 1);
    }

    pub fn extend<I: IntoIterator<Item = MatchEntry<'a>>>(&mut self, query: &WordQuery, it: I) {
        for match_entry in it {
            self.add(query, match_entry)
        }
    }

    pub fn get(&self, word: *const str) -> Option<&IndexToQueryEntry<'a>> {
        self.map.get(&word)
    }

    pub fn query_len(&self) -> usize {
        self.query_len
    }
}

pub(crate) struct IndexToQueryEntry<'a> {
    pub distance: MatchDistance,
    pub docs: &'a RoaringBitmap,
    pub query_index: usize,
}
