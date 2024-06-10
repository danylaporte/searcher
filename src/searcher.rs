use crate::{
    AttrProps, Direction, DocId, Index, IndexLog, IndexResults, IndexToQuery, MatchEntry, Presence,
    SearchQuery, SearchResults,
};
use indexmap::IndexMap;
use once_cell::sync::OnceCell;
use roaring::RoaringBitmap;
use std::mem::take;

type DirectionIndex = (Direction, usize);

pub struct Searcher {
    attrs: IndexMap<Box<str>, Attr, fxhash::FxBuildHasher>,
    attrs_priorities: OnceCell<Vec<(u8, Vec<DirectionIndex>)>>,
    backward: Index,
    forward: Index,
    index_log: IndexLog,
}

impl Searcher {
    pub fn new() -> Self {
        Searcher {
            attrs: Default::default(),
            attrs_priorities: Default::default(),
            backward: Index::new(Direction::Backward),
            forward: Index::new(Direction::Forward),
            index_log: IndexLog::default(),
        }
    }

    pub(crate) fn attrs_priorities(&self) -> &[(u8, Vec<DirectionIndex>)] {
        self.attrs_priorities.get_or_init(|| {
            let mut map = IndexMap::<u8, Vec<DirectionIndex>, fxhash::FxBuildHasher>::default();

            self.attrs.values().for_each(|a| {
                map.entry(a.priority)
                    .or_default()
                    .push((a.direction, a.index))
            });

            map.sort_unstable_keys();
            map.into_iter().collect()
        })
    }

    fn direction_index_mut(&mut self, direction: Direction) -> &mut Index {
        match direction {
            Direction::Forward => &mut self.forward,
            Direction::Backward => &mut self.backward,
        }
    }

    pub fn insert_doc_attribute(&mut self, doc_id: DocId, name: &str, value: &str) {
        if let Some(a) = self.attrs.get(name) {
            let mut log = take(&mut self.index_log);
            let direction = a.direction;
            let attr_index = a.index;

            self.direction_index_mut(direction)
                .insert_doc_attribute(doc_id, attr_index, value, &mut log);

            self.index_log = log;
        }
    }

    pub fn query<'a>(&'a self, query: &SearchQuery) -> SearchResults<'a> {
        let mut backward_temp = Vec::new();
        let mut forward_temp = Vec::new();
        let mut backward_query = IndexToQuery::default();
        let mut forward_query = IndexToQuery::default();
        let mut required = None;
        let mut denied = RoaringBitmap::new();
        let mut optional = RoaringBitmap::new();

        for q in &query.words {
            self.forward.query(q, &mut forward_temp);
            self.backward.query(q, &mut backward_temp);

            match q.presence {
                Presence::Optional => {
                    add_entries(&mut optional, &forward_temp);
                    add_entries(&mut optional, &backward_temp);
                }
                Presence::Denied => {
                    add_entries(&mut denied, &forward_temp);
                    add_entries(&mut denied, &backward_temp);
                }
                Presence::Required => {
                    if forward_temp.is_empty() && backward_temp.is_empty() {
                        required = Some(RoaringBitmap::new());
                        break;
                    }

                    if required.is_none() {
                        required = Some(RoaringBitmap::full());
                    }

                    if let Some(r) = required.as_mut() {
                        intersect_entries(r, &forward_temp);
                        intersect_entries(r, &backward_temp);
                    }
                }
            }

            backward_query.extend(q, backward_temp.drain(..));
            forward_query.extend(q, forward_temp.drain(..));
        }

        let mut doc_ids = if optional.is_empty() {
            required.unwrap_or_default()
        } else if let Some(r) = required {
            optional & r
        } else {
            optional
        };

        doc_ids -= denied;

        let backward_results = IndexResults {
            index: &self.backward,
            index_to_query: backward_query,
        };

        let forward_results = IndexResults {
            index: &self.forward,
            index_to_query: forward_query,
        };

        SearchResults::new(backward_results, doc_ids, forward_results, self)
    }

    fn reindex_attribute(&mut self, direction: Direction) {
        self.attrs
            .values_mut()
            .filter(|a| a.direction == direction)
            .enumerate()
            .for_each(|(index, a)| a.index = index);

        self.attrs_priorities = OnceCell::new();
    }

    pub fn remove_attr(&mut self, name: &str) -> bool {
        let mut log = IndexLog::default();

        match self.attrs.shift_remove(name) {
            Some(a) => {
                self.direction_index_mut(a.direction)
                    .remove_attr(a.index, &mut log);
                self.reindex_attribute(a.direction);

                true
            }
            None => false,
        }
    }

    pub fn remove_doc(&mut self, doc_id: DocId) {
        self.backward.remove_doc(doc_id);
        self.forward.remove_doc(doc_id);
    }

    pub fn set_attribute(&mut self, name: String, attr: AttrProps) -> bool {
        if self.attrs.contains_key(name.as_str()) {
            false
        } else {
            let direction = attr.direction.unwrap_or_default();

            self.attrs.insert(
                name.into_boxed_str(),
                Attr {
                    direction: attr.direction.unwrap_or_default(),
                    priority: attr.priority.unwrap_or_default(),
                    index: 0,
                },
            );

            self.reindex_attribute(direction);

            true
        }
    }
}

impl Default for Searcher {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

struct Attr {
    direction: Direction,
    priority: u8,
    index: usize,
}

fn add_entries(denied: &mut RoaringBitmap, entries: &[MatchEntry]) {
    for entry in entries {
        *denied |= &entry.entry.docs;
    }
}

fn intersect_entries(required: &mut RoaringBitmap, entries: &[MatchEntry]) {
    for entry in entries {
        *required &= &entry.entry.docs;
    }
}
