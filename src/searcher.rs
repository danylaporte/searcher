use crate::{
    AttrProps, Direction, DocId, Index, IndexLog, IndexResults, IndexToQuery, MatchEntry, Presence,
    SearchQuery, SearchResults,
};
use indexmap::IndexMap;
use once_cell::sync::OnceCell;
use roaring::RoaringBitmap;

pub type AttrMap = IndexMap<Box<str>, Attr, fxhash::FxBuildHasher>;
type DirectionIndex = (Direction, usize);
type PriorityDirectionIndexes = (u8, Vec<DirectionIndex>);

pub struct Searcher {
    attrs: AttrMap,
    attrs_priorities: OnceCell<Vec<Vec<PriorityDirectionIndexes>>>,
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

    pub(crate) fn attrs(&self) -> &AttrMap {
        &self.attrs
    }

    pub(crate) fn attrs_priorities(&self, culture: u8) -> &[PriorityDirectionIndexes] {
        let by_cultures = self.attrs_priorities.get_or_init(|| {
            let count = self
                .attrs
                .values()
                .filter_map(|a| a.culture)
                .max()
                .unwrap_or_default()
                + 1;
            (0..count)
                .map(|culture| self.compute_attr_priorities(culture))
                .collect::<Vec<_>>()
        });

        by_cultures
            .get(culture as usize)
            .or_else(|| by_cultures.first())
            .map_or(&[], |v| v)
    }

    fn compute_attr_priorities(&self, culture: u8) -> Vec<PriorityDirectionIndexes> {
        let mut map = IndexMap::<u8, Vec<DirectionIndex>, fxhash::FxBuildHasher>::default();

        self.attrs
            .values()
            .filter(|a| a.culture.map_or(true, |c| c == culture))
            .for_each(|a| {
                map.entry(a.priority)
                    .or_default()
                    .push((a.direction, a.index))
            });

        map.sort_unstable_keys();
        map.into_iter().collect()
    }

    /// Gets the words inside a doc attribute as they are indexed.
    pub fn get_doc_attr_words<'a>(
        &'a self,
        doc_id: DocId,
        name: &str,
    ) -> impl Iterator<Item = &'a str> {
        self.get_doc_attr_words_impl(doc_id, name)
            .iter()
            .map(|w| unsafe { &**w })
    }

    fn get_doc_attr_words_impl<'a>(&'a self, doc_id: DocId, name: &str) -> &'a [*const str] {
        let Some(a) = self.attrs.get(name) else {
            return &[];
        };

        let index = match a.direction {
            Direction::Backward => &self.backward,
            Direction::Forward => &self.forward,
        };

        index.get_doc_attribute_words(doc_id, a.index)
    }

    pub fn insert_doc_attribute(&mut self, doc_id: DocId, name: &str, value: &str) {
        if let Some(a) = self.attrs.get(name) {
            direction_index_mut(a.direction, &mut self.backward, &mut self.forward)
                .insert_doc_attribute(doc_id, a.index, value, &mut self.index_log, &self.attrs);
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
            self.forward.query(q, query.culture, &mut forward_temp);
            self.backward.query(q, query.culture, &mut backward_temp);

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

        SearchResults::new(
            backward_results,
            query.culture,
            doc_ids,
            forward_results,
            self,
        )
    }

    fn reindex_attribute(&mut self, direction: Direction) {
        self.attrs
            .values_mut()
            .filter(|a| a.direction == direction)
            .enumerate()
            .for_each(|(index, a)| a.index = index);

        self.attrs_priorities = OnceCell::new();

        direction_index_mut(direction, &mut self.backward, &mut self.forward)
            .ensure_culture(&self.attrs, &mut self.index_log);
    }

    pub fn remove_attr(&mut self, name: &str) -> bool {
        let mut log = IndexLog::default();

        match self.attrs.shift_remove(name) {
            Some(a) => {
                direction_index_mut(a.direction, &mut self.backward, &mut self.forward)
                    .swap_remove_attr(a.index, a.culture, &mut log);

                self.reindex_attribute(a.direction);
                true
            }
            None => false,
        }
    }

    pub fn remove_doc(&mut self, doc_id: DocId) {
        self.backward.remove_doc(doc_id, &mut self.index_log);
        self.forward.remove_doc(doc_id, &mut self.index_log);
    }

    pub fn set_attribute(&mut self, name: String, attr: AttrProps) -> bool {
        if self.attrs.contains_key(name.as_str()) {
            false
        } else {
            self.attrs.insert(
                name.into_boxed_str(),
                Attr {
                    culture: attr.culture,
                    direction: attr.direction,
                    priority: attr.priority,
                    index: 0,
                },
            );

            self.reindex_attribute(attr.direction);
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

pub(crate) struct Attr {
    pub(crate) direction: Direction,
    pub(crate) culture: Option<u8>,
    pub(crate) priority: u8,
    pub(crate) index: usize,
}

fn add_entries(denied: &mut RoaringBitmap, entries: &[MatchEntry]) {
    for entry in entries {
        *denied |= entry.docs;
    }
}

fn direction_index_mut<'a>(
    direction: Direction,
    backward: &'a mut Index,
    forward: &'a mut Index,
) -> &'a mut Index {
    match direction {
        Direction::Forward => forward,
        Direction::Backward => backward,
    }
}

fn intersect_entries(required: &mut RoaringBitmap, entries: &[MatchEntry]) {
    for entry in entries {
        *required &= entry.docs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_remove_backward() {
        let mut searcher = Searcher::new();

        searcher.set_attribute(
            "*".into(),
            AttrProps::default().direction(Direction::Backward),
        );

        searcher.insert_doc_attribute(DocId::from(0), "*", "balance balle");
        searcher.insert_doc_attribute(DocId::from(1), "*", "air");

        assert_eq!(2, searcher.backward.docs().len());
        assert_eq!(1, searcher.backward.per_culture().len());

        searcher.insert_doc_attribute(DocId::from(0), "*", "balance balle");
        searcher.insert_doc_attribute(DocId::from(1), "*", "air");

        assert_eq!(2, searcher.backward.docs().len());
        assert_eq!(1, searcher.backward.per_culture().len());
        assert_eq!(3, searcher.backward.per_culture()[0].len());
        assert_eq!(3, searcher.backward.word_intern().len());

        searcher.remove_doc(DocId::from(0));

        assert_eq!(2, searcher.backward.docs().len());
        assert!(searcher.backward.docs()[0].attrs.is_empty());
        assert_eq!(1, searcher.backward.per_culture()[0].len());
        assert_eq!(1, searcher.backward.word_intern().len());

        searcher.insert_doc_attribute(DocId::from(0), "*", "air balle");

        assert_eq!(2, searcher.backward.docs().len());
        assert!(!searcher.backward.docs()[0].attrs.is_empty());
        assert_eq!(searcher.backward.word_intern(), (vec!["ellab", "ria"]));
        assert_eq!(2, searcher.backward.per_culture()[0].len());

        searcher.remove_doc(DocId::from(1));

        assert_eq!(2, searcher.backward.docs().len());
        assert!(searcher.backward.docs()[1].attrs.is_empty());
        assert_eq!(searcher.backward.word_intern(), (vec!["ellab", "ria"]));
        assert_eq!(2, searcher.backward.per_culture()[0].len());
    }

    #[test]
    fn insert_remove_culture() {
        let mut searcher = Searcher::new();
        searcher.set_attribute("en".into(), AttrProps::default().culture(0));
        searcher.set_attribute("fr".into(), AttrProps::default().culture(1));

        assert_eq!(2, searcher.attrs.len());
        assert_eq!(2, searcher.forward.per_culture().len());

        searcher.insert_doc_attribute(DocId::from(0), "en", "balance");
        searcher.insert_doc_attribute(DocId::from(1), "en", "total");

        searcher.insert_doc_attribute(DocId::from(0), "fr", "encours");
        searcher.insert_doc_attribute(DocId::from(1), "fr", "total");

        assert_eq!(2, searcher.forward.docs().len());
        assert_eq!(2, searcher.forward.per_culture()[0].len());

        assert!(searcher
            .query(&SearchQuery::new(0, "balance"))
            .contains_doc_id(DocId::from(0)));

        assert!(searcher
            .query(&SearchQuery::new(1, "encours"))
            .contains_doc_id(DocId::from(0)));

        assert!(!searcher
            .query(&SearchQuery::new(1, "balance"))
            .contains_doc_id(DocId::from(0)));

        assert!(!searcher
            .query(&SearchQuery::new(0, "encours"))
            .contains_doc_id(DocId::from(0)));

        searcher.remove_attr("fr");

        assert_eq!(1, searcher.attrs.len());
        assert_eq!(2, searcher.forward.docs().len());
        assert_eq!(1, searcher.forward.per_culture().len());
        assert_eq!(2, searcher.forward.per_culture()[0].len());
        assert_eq!(searcher.forward.word_intern(), vec!["balance", "total"]);
    }

    #[test]
    fn search_with_and_without_culture() {
        let mut searcher = Searcher::new();
        searcher.set_attribute("en".into(), AttrProps::default().culture(0));
        searcher.set_attribute("fr".into(), AttrProps::default().culture(1));
        searcher.set_attribute("*".into(), AttrProps::default());

        assert_eq!(3, searcher.attrs.len());
        assert_eq!(2, searcher.forward.per_culture().len());

        searcher.insert_doc_attribute(DocId::from(0), "en", "balance");
        searcher.insert_doc_attribute(DocId::from(1), "en", "total");

        searcher.insert_doc_attribute(DocId::from(0), "fr", "encours");
        searcher.insert_doc_attribute(DocId::from(1), "fr", "total");

        searcher.insert_doc_attribute(DocId::from(0), "*", "test");
        searcher.insert_doc_attribute(DocId::from(1), "*", "app");

        assert_eq!(2, searcher.forward.docs().len());
        assert_eq!(4, searcher.forward.per_culture()[0].len());
        assert_eq!(4, searcher.forward.per_culture()[1].len());
        assert_eq!(
            searcher.forward.word_intern(),
            vec!["app", "balance", "encours", "test", "total"]
        );

        assert!(searcher
            .query(&SearchQuery::new(1, "test"))
            .contains_doc_id(DocId::from(0)));

        assert!(searcher
            .query(&SearchQuery::new(0, "app"))
            .contains_doc_id(DocId::from(1)));

        searcher.remove_attr("*");

        assert_eq!(2, searcher.attrs.len());
        assert_eq!(2, searcher.forward.docs().len());
        assert_eq!(2, searcher.forward.per_culture().len());
        assert_eq!(2, searcher.forward.per_culture()[0].len());

        assert_eq!(
            searcher.forward.word_intern(),
            vec!["balance", "encours", "total"]
        );
    }
}
