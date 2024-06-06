use crate::{Direction, DocId, MatchDistance, MatchEntry, WordQuery, WordQueryOp};
use fxhash::FxBuildHasher;
use indexmap::{IndexMap, IndexSet};
use levenshtein_automata::Distance;
use roaring::RoaringBitmap;
use std::{cmp::min, iter::Peekable, mem::take, str::Chars};
use str_utils::char_map::lower_no_accent_char;

#[derive(Default)]
struct Doc {
    attrs: Box<[DocAttr]>,
}

impl Doc {
    fn contains_word(&self, word: *const str) -> bool {
        self.attrs.iter().any(|a| a.words.contains(&word))
    }
}

#[derive(Default)]
struct DocAttr {
    words: Box<[*const str]>,
}

impl Doc {
    fn ensure_attrs_size(&mut self, attribute_count: usize) {
        if self.attrs.len() < attribute_count {
            let mut vec = take(&mut self.attrs).into_vec();
            let range = vec.len()..attribute_count;

            vec.extend(range.map(|_| Default::default()));

            self.attrs = vec.into_boxed_slice();
        }
    }

    fn remove_attr(&mut self, attribute_index: usize) -> Option<DocAttr> {
        if self.attrs.len() > attribute_index {
            let mut vec = take(&mut self.attrs).into_vec();
            let old = vec.remove(attribute_index);

            self.attrs = vec.into_boxed_slice();

            Some(old)
        } else {
            None
        }
    }
}

pub(crate) struct Entry {
    pub docs: RoaringBitmap,
    len: u8,
}

pub(crate) struct Index {
    direction: Direction,
    docs: Vec<Doc>,
    map: IndexMap<Box<str>, Entry, FxBuildHasher>,
}

impl Index {
    pub(crate) fn new(direction: Direction) -> Self {
        Self {
            direction,
            map: Default::default(),
            docs: Vec::new(),
        }
    }

    fn add_word_doc(&mut self, word: &str, doc_id: DocId, s: &mut String) -> *const str {
        let r = match self.direction {
            Direction::Backward => {
                s.clear();
                s.extend(word.chars().rev());
                &**s
            }
            Direction::Forward => word,
        };

        let out = match self.map.get_full_mut(r) {
            Some((_, k, doc)) => {
                doc.docs.insert(doc_id.0);
                &**k
            }
            None => {
                let mut docs = RoaringBitmap::new();

                docs.insert(doc_id.0);

                let len = min(r.chars().count(), u8::MAX as usize) as u8;
                let key = r.to_string().into_boxed_str();
                let ptr: *const str = &*key;

                self.map.insert_sorted(key, Entry { docs, len });

                ptr
            }
        };

        s.clear();

        out
    }

    fn binary_search_word(&self, word: &str) -> Result<usize, usize> {
        self.map.binary_search_by_key(&word, |k, _| &**k)
    }

    fn binary_search_word_full(&self, word: &str) -> usize {
        match self.binary_search_word(word) {
            Ok(index) => index,
            Err(index) => index,
        }
    }

    /// Removes all docs that do not contains the word.
    /// If the word is no more associated with a document, the word is removed.
    fn clean_word(&mut self, word: &str, log_docs: &mut Vec<u32>) {
        let Some((_, w, entry)) = self.map.get_full_mut(word) else {
            return;
        };

        let doc_ids_to_remove = entry.docs.iter().filter(|doc_id| {
            self.docs
                .get(*doc_id as usize)
                .map_or(true, |doc| !doc.contains_word(&**w))
        });

        log_docs.extend(doc_ids_to_remove);

        log_docs.iter().for_each(|id| {
            entry.docs.remove(*id);
        });

        log_docs.clear();

        if entry.docs.is_empty() {
            self.map.shift_remove(word);
        }
    }

    fn contains<'a>(&'a self, q: &WordQuery, out: &mut Vec<MatchEntry<'a>>) {
        let word = q.directional_word(self.direction);
        self.find_exact(q, 0, |w| w.contains(word), out)
    }

    fn ends_with<'a>(&'a self, q: &WordQuery, out: &mut Vec<MatchEntry<'a>>) {
        match self.direction {
            Direction::Forward => self.find_exact(q, 0, |word| word.ends_with(&*q.word), out),
            Direction::Backward => {
                let term = q.backward_word();
                let index = self.binary_search_word_full(term);

                self.find_exact(q, index, |indexed| indexed.starts_with(term), out)
            }
        }
    }

    fn ensure_docs_size(&mut self, doc_id: DocId) {
        let len = doc_id.index() + 1;
        let range = self.docs.len()..len;

        self.docs.extend(range.map(|_| Default::default()));
    }

    fn eq<'a>(&'a self, query: &WordQuery, out: &mut Vec<MatchEntry<'a>>) {
        let word = query.directional_word(self.direction);

        if let Some((word, entry)) = self.map.get_key_value(word) {
            out.push(MatchEntry {
                distance: MatchDistance::Exact(0),
                entry,
                word,
            });
        }
    }

    fn find<'a>(
        &'a self,
        start: usize,
        matcher: impl Fn(&str, &Entry) -> Option<MatchDistance>,
        out: &mut Vec<MatchEntry<'a>>,
    ) {
        let iter = self.map.as_slice()[start..]
            .iter()
            .filter_map(|(word, entry)| {
                matcher(word, entry).map(|distance| MatchEntry {
                    distance,
                    entry,
                    word,
                })
            });

        out.extend(iter);
    }

    fn find_exact<'a>(
        &'a self,
        query: &WordQuery,
        start: usize,
        matcher: impl Fn(&str) -> bool,
        out: &mut Vec<MatchEntry<'a>>,
    ) {
        self.find(
            start,
            |word, data| {
                if matcher(word) {
                    Some(MatchDistance::Exact(data.len - query.len))
                } else {
                    None
                }
            },
            out,
        )
    }

    fn fuzzy<'a>(&'a self, query: &WordQuery, out: &mut Vec<MatchEntry<'a>>) {
        match query.directional_dfa(self.direction) {
            Some(dfa) => self.find(
                0,
                |word, data| match dfa.eval(word) {
                    Distance::AtLeast(_) => None,
                    Distance::Exact(0) => Some(MatchDistance::Exact(
                        data.len.saturating_sub(query.len) + query.len.saturating_sub(data.len),
                    )),
                    Distance::Exact(n) => Some(MatchDistance::Fuzzy(
                        (data.len.saturating_sub(query.len) + query.len.saturating_sub(data.len))
                            .saturating_add(n),
                    )),
                },
                out,
            ),
            None => self.starts_with(query, out),
        }
    }

    pub(crate) fn get_doc_attribute_words(&self, id: DocId, attr_index: usize) -> &[*const str] {
        match self.docs.get(id.index()) {
            Some(doc) => match doc.attrs.get(attr_index) {
                Some(attr) => &attr.words,
                None => Default::default(),
            },
            None => Default::default(),
        }
    }

    pub(crate) fn insert_doc_attribute(
        &mut self,
        doc_id: DocId,
        attribute_index: usize,
        value: &str,
        log: &mut IndexLog,
    ) {
        fn find_next_word(chars: &mut Peekable<Chars>, word: &mut String) {
            #[derive(Clone, Copy)]
            enum CharKind {
                Whitespace,
                Alpha,
                Number,
            }

            let mut kind = CharKind::Whitespace;

            word.clear();

            while let Some(&c) = chars.peek() {
                if c.is_alphabetic() {
                    if !matches!(kind, CharKind::Alpha | CharKind::Whitespace) {
                        break;
                    }

                    word.extend(lower_no_accent_char(c));
                    kind = CharKind::Alpha;
                } else if c.is_numeric() {
                    if !matches!(kind, CharKind::Number | CharKind::Whitespace) {
                        break;
                    }

                    word.push(c);
                    kind = CharKind::Number;
                } else if c == '#' || c == '°' {
                    if matches!(kind, CharKind::Whitespace) {
                        chars.next();
                        word.push(c);
                    }

                    break;
                } else if !word.is_empty() {
                    chars.next();
                    break;
                }

                chars.next();
            }
        }

        let mut word = String::new();
        let mut new_word_list = Vec::new();
        let mut chars = value.chars().peekable();

        loop {
            find_next_word(&mut chars, &mut word);

            if word.is_empty() {
                break;
            }

            new_word_list.push(self.add_word_doc(&word, doc_id, &mut log.str));
        }

        let doc = match self.docs.get_mut(doc_id.index()) {
            Some(doc) => doc,
            None => {
                // no need to add a document if the new value is empty.
                if new_word_list.is_empty() {
                    return;
                }

                self.ensure_docs_size(doc_id);
                self.docs.get_mut(doc_id.index()).expect("doc")
            }
        };

        let doc_attr = match doc.attrs.get_mut(attribute_index) {
            Some(attr) => attr,
            None => {
                // no need to add an attribute if the new value is empty.
                if new_word_list.is_empty() {
                    return;
                }

                doc.ensure_attrs_size(attribute_index + 1);
                doc.attrs.get_mut(attribute_index).expect("doc_attr")
            }
        };

        log.words.extend(doc_attr.words.iter().copied());

        new_word_list.iter().for_each(|w| {
            log.words.swap_remove(w);
        });

        doc_attr.words = new_word_list.into_boxed_slice();

        log.words
            .iter()
            .for_each(|word| self.clean_word(unsafe { &**word }, &mut log.docs));

        log.words.clear();
    }

    pub(crate) fn query<'a>(&'a self, q: &WordQuery, out: &mut Vec<MatchEntry<'a>>) {
        match q.op {
            WordQueryOp::Contains => self.contains(q, out),
            WordQueryOp::EndsWith => self.ends_with(q, out),
            WordQueryOp::Eq => self.eq(q, out),
            WordQueryOp::Fuzzy => self.fuzzy(q, out),
            WordQueryOp::StartsWith => self.starts_with(q, out),
        }
    }

    pub(crate) fn remove_attr(&mut self, attribute_index: usize, log: &mut IndexLog) {
        self.docs.iter_mut().for_each(|d| {
            if let Some(doc_attr) = d.remove_attr(attribute_index) {
                log.words.extend(doc_attr.words.iter().copied());
            }
        });

        log.words.sort_unstable_by(|a, b| b.cmp(a));

        log.words
            .iter()
            .for_each(|w| self.clean_word(unsafe { &**w }, &mut log.docs));

        log.words.clear();
    }

    pub(crate) fn remove_doc(&mut self, doc_id: DocId) {
        let removed_index = doc_id.index();

        if self.docs.len() <= removed_index {
            return;
        }

        let last_index = self.docs.len() - 1;

        self.docs.swap_remove(removed_index);

        if last_index > removed_index {
            self.map.retain(|_, entry| {
                if entry.docs.remove(last_index as u32) {
                    entry.docs.insert(removed_index as u32);
                    true
                } else {
                    entry.docs.remove(removed_index as u32);
                    !entry.docs.is_empty()
                }
            });
        } else {
            self.map.retain(|_, entry| {
                !entry.docs.remove(removed_index as u32) || !entry.docs.is_empty()
            });
        }
    }

    fn starts_with<'a>(&'a self, q: &WordQuery, out: &mut Vec<MatchEntry<'a>>) {
        match self.direction {
            Direction::Forward => {
                let index = self.binary_search_word_full(&q.word);
                self.find_exact(q, index, |word| word.starts_with(&*q.word), out)
            }
            Direction::Backward => {
                let word = q.backward_word();
                self.find_exact(q, 0, |indexed| indexed.ends_with(word), out)
            }
        }
    }
}

unsafe impl Send for Index {}
unsafe impl Sync for Index {}

#[derive(Default)]
pub(crate) struct IndexLog {
    docs: Vec<u32>,
    str: String,
    words: IndexSet<*const str, fxhash::FxBuildHasher>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Presence;

    fn create_index(docs: &[&'static str], direction: Direction) -> Index {
        let mut index = Index::new(direction);
        let mut log = IndexLog::default();

        docs.iter().enumerate().for_each(|(doc_id, doc)| {
            index.insert_doc_attribute(DocId(doc_id as u32), 0, doc, &mut log)
        });

        index
    }

    fn create_query(word: &str, op: WordQueryOp, presence: Presence) -> WordQuery {
        WordQuery::new(word.into(), op, presence, 0)
    }

    #[test]
    fn contains_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("oun", WordQueryOp::Contains, Presence::Optional),
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance::Exact(6), "seirtnuoc"),
                (MatchDistance::Exact(4), "yrtnuoc")
            ]
        );
    }

    #[test]
    fn contains_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("oun", WordQueryOp::Contains, Presence::Optional),
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance::Exact(6), "countries"),
                (MatchDistance::Exact(4), "country")
            ]
        );
    }

    #[test]
    fn ends_with_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("try", WordQueryOp::EndsWith, Presence::Optional),
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance::Exact(4), "yrtnuoc")]);
    }

    #[test]
    fn ends_with_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("try", WordQueryOp::EndsWith, Presence::Optional),
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance::Exact(4), "country")]);
    }

    #[test]
    fn eq_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("country", WordQueryOp::Eq, Presence::Optional),
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance::Exact(0), "yrtnuoc")]);
    }

    #[test]
    fn eq_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("country", WordQueryOp::Eq, Presence::Optional),
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance::Exact(0), "country")]);
    }

    #[test]
    fn fuzzy_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("country", WordQueryOp::Fuzzy, Presence::Optional),
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance::Exact(0), "yrtnuoc")]);
    }

    #[test]
    fn fuzzy_forward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("country", WordQueryOp::Fuzzy, Presence::Optional),
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance::Fuzzy(3), "countries"),
                (MatchDistance::Exact(0), "country")
            ]
        );
    }

    #[test]
    fn fuzzy_longer_query() {
        let mut matches = Vec::new();
        let index = create_index(&["country"], Direction::Forward);

        index.query(
            &create_query("countries", WordQueryOp::Fuzzy, Presence::Optional),
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance::Fuzzy(5), "country"),]);
    }

    #[test]
    fn fuzzy_3_letter_query() {
        let mut matches = Vec::new();
        let index = create_index(&["dmo"], Direction::Forward);

        index.query(
            &create_query("dmo", WordQueryOp::Fuzzy, Presence::Optional),
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance::Exact(0), "dmo"),]);
    }

    #[test]
    fn remove_first_doc() {
        let mut index = create_index(&["air canada", "air france"], Direction::Forward);

        index.remove_doc(DocId(0));

        assert_eq!(index.docs.len(), 1);

        assert_eq!(
            index.map.keys().map(|s| &**s).collect::<Vec<_>>(),
            vec!["air", "france"]
        );

        assert_eq!(
            index
                .map
                .iter()
                .fold(RoaringBitmap::new(), |acc, entry| acc | &entry.1.docs)
                .into_iter()
                .collect::<Vec<_>>(),
            vec![0]
        );

        assert_eq!(
            index.docs[0].attrs[0]
                .words
                .iter()
                .map(|s| unsafe { &**s })
                .collect::<Vec<&str>>(),
            vec!["air", "france"]
        );
    }

    #[test]
    fn remove_last_doc() {
        let mut index = create_index(&["air canada", "air france"], Direction::Forward);

        index.remove_doc(DocId(1));

        assert_eq!(index.docs.len(), 1);

        assert_eq!(
            index.map.keys().map(|s| &**s).collect::<Vec<_>>(),
            vec!["air", "canada"]
        );

        assert_eq!(
            index
                .map
                .iter()
                .fold(RoaringBitmap::new(), |acc, entry| acc | &entry.1.docs)
                .into_iter()
                .collect::<Vec<_>>(),
            vec![0]
        );

        assert_eq!(
            index.docs[0].attrs[0]
                .words
                .iter()
                .map(|s| unsafe { &**s })
                .collect::<Vec<&str>>(),
            vec!["air", "canada"]
        );
    }

    #[test]
    fn starts_with_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("coun", WordQueryOp::StartsWith, Presence::Optional),
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance::Exact(5), "seirtnuoc"),
                (MatchDistance::Exact(3), "yrtnuoc")
            ]
        );
    }

    #[test]
    fn starts_with_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("coun", WordQueryOp::StartsWith, Presence::Optional),
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance::Exact(5), "countries"),
                (MatchDistance::Exact(3), "country")
            ]
        );
    }
}
