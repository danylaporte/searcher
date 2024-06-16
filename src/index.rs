use crate::{
    AttrMap, Direction, DocId, MatchEntry, StrIntern, WordIndex, WordInternResolver, WordQuery,
    WordQueryOp,
};
use fxhash::FxHashSet;
use std::{iter::Peekable, mem::take, str::Chars};
use str_utils::char_map::lower_no_accent_char;

#[derive(Default)]
pub(crate) struct Doc {
    pub(crate) attrs: Box<[DocAttr]>,
}

impl Doc {
    fn contains_word(&self, word: *const str) -> bool {
        self.attrs.iter().any(|a| a.words.contains(&word))
    }

    fn ensure_attrs_size(&mut self, attribute_count: usize) {
        if self.attrs.len() < attribute_count {
            let mut vec = take(&mut self.attrs).into_vec();
            let range = vec.len()..attribute_count;

            vec.extend(range.map(|_| Default::default()));

            self.attrs = vec.into_boxed_slice();
        }
    }

    fn swap_remove_attr(&mut self, attribute_index: usize) -> Option<DocAttr> {
        if self.attrs.len() > attribute_index {
            let mut vec = take(&mut self.attrs).into_vec();
            let old = vec.swap_remove(attribute_index);

            self.attrs = vec.into_boxed_slice();

            Some(old)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub(crate) struct DocAttr {
    words: Box<[*const str]>,
}

pub(crate) struct Index {
    direction: Direction,
    docs: Vec<Doc>,
    per_culture: Vec<WordIndex>,
    word_intern: StrIntern,
}

impl Index {
    pub(crate) const fn new(direction: Direction) -> Self {
        Self {
            direction,
            docs: Vec::new(),
            per_culture: Vec::new(),
            word_intern: StrIntern::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn docs(&self) -> &[Doc] {
        &self.docs
    }

    pub(crate) fn ensure_culture(&mut self, attrs: &AttrMap, log: &mut IndexLog) {
        if !attrs.values().any(|a| a.direction == self.direction) {
            self.per_culture = Vec::new();
            return;
        }

        let count = attrs
            .values()
            .filter_map(|a| a.culture)
            .max()
            .unwrap_or_default() as usize
            + 1;

        if self.per_culture.len() == count {
            return;
        }

        if self.per_culture.len() > count {
            while self.per_culture.len() > count {
                self.per_culture.pop();
            }
            return;
        }

        let is_empty = self.per_culture.is_empty();
        let range = self.per_culture.len()..count;

        self.per_culture
            .extend(range.clone().map(|_| WordIndex::new()));

        if is_empty {
            return;
        }

        let attr_indexes = attrs
            .values()
            .filter(|a| a.culture.is_none() && a.direction == self.direction)
            .map(|a| a.index)
            .collect::<Vec<_>>();

        if attr_indexes.is_empty() {
            return;
        }

        let word_indexes = self.per_culture.get_mut(range).expect("word_indexes");

        for (index, doc) in self.docs.iter().enumerate() {
            log.words.clear();

            for attr_index in &attr_indexes {
                if let Some(attr) = doc.attrs.get(*attr_index) {
                    log.words.extend(attr.words.iter().copied());
                }
            }

            let doc_id = DocId::from(index as u32);

            for word in &log.words {
                let word = unsafe { &**word };

                for wi in &mut *word_indexes {
                    wi.insert_word_doc(word, WordInternResolver::StaticWord(word), doc_id);
                }
            }
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
        attrs: &AttrMap,
    ) {
        let Some((_, a)) = attrs.get_index(attribute_index) else {
            return;
        };

        let word_indexes = word_indexes(a.culture, &mut self.per_culture);

        let new_word_list = insert_doc_word_list(
            word_indexes,
            value,
            doc_id,
            log,
            &mut self.word_intern,
            self.direction,
        );

        let doc = match self.docs.get_mut(doc_id.index()) {
            Some(doc) => doc,
            None => {
                // no need to add a document if the new value is empty.
                if new_word_list.is_empty() {
                    return;
                }

                ensure_size(&mut self.docs, doc_id.0 as usize, Default::default);
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

        log.words.clear();
        log.words.extend(doc_attr.words.iter().copied());

        new_word_list.iter().for_each(|w| {
            log.words.remove(w);
        });

        doc_attr.words = new_word_list.into_boxed_slice();
        log.words.retain(|w| !doc.contains_word(*w));

        self.remove_words_doc(&log.words, doc_id);
    }

    #[cfg(test)]
    pub(crate) fn per_culture(&self) -> &[WordIndex] {
        &self.per_culture
    }

    pub(crate) fn query<'a>(&'a self, q: &WordQuery, culture: u8, out: &mut Vec<MatchEntry<'a>>) {
        let culture = culture as usize;

        if let Some(word_index) = self
            .per_culture
            .get(culture)
            .or_else(|| self.per_culture.first())
        {
            match q.op {
                WordQueryOp::Contains => contains(self.direction, word_index, q, out),
                WordQueryOp::EndsWith => ends_with(self.direction, word_index, q, out),
                WordQueryOp::Eq => word_index.eq(q.directional_word(self.direction), out),
                WordQueryOp::Fuzzy => fuzzy(self.direction, word_index, q, out),
                WordQueryOp::StartsWith => starts_with(self.direction, word_index, q, out),
            }
        }
    }

    pub(crate) fn remove_doc(&mut self, doc_id: DocId, log: &mut IndexLog) {
        let Some(doc) = self.docs.get_mut(doc_id.0 as usize) else {
            return;
        };

        // replace the doc with a default empty doc.
        let doc = take(doc);

        log.words.clear();

        for doc_attr in &*doc.attrs {
            log.words.extend(doc_attr.words.iter().copied());
        }

        self.remove_words_doc(&log.words, doc_id);
    }

    fn remove_word_doc(&mut self, word: *const str, doc_id: DocId) {
        let word = unsafe { &*word };
        let mut word_to_delete = true;

        for word_index in &mut self.per_culture {
            word_to_delete = word_index.remove_word_doc(word, doc_id) && word_to_delete;
        }

        if word_to_delete {
            self.word_intern.remove(word);
        }
    }

    fn remove_words_doc(&mut self, words: &FxHashSet<*const str>, doc_id: DocId) {
        for word in words {
            self.remove_word_doc(*word, doc_id);
        }
    }

    pub(crate) fn swap_remove_attr(
        &mut self,
        attribute_index: usize,
        culture: Option<u8>,
        log: &mut IndexLog,
    ) {
        log.words.clear();

        let fast_delete = culture.is_none();
        let word_indexes = word_indexes(culture, &mut self.per_culture);

        for (doc_index, d) in self.docs.iter_mut().enumerate() {
            let Some(doc_attr) = d.swap_remove_attr(attribute_index) else {
                continue;
            };

            let doc_id = DocId::from(doc_index as u32);
            let mut words = doc_attr.words.into_vec();

            words.sort_unstable();
            words.dedup();
            words.reverse();

            for word in words {
                if d.contains_word(word) {
                    continue;
                }

                let s = unsafe { &*word };
                let check_to_clean = word_indexes
                    .iter_mut()
                    .fold(true, |d, wi| wi.remove_word_doc(s, doc_id) && d);

                if check_to_clean {
                    if fast_delete {
                        self.word_intern.remove(s);
                    } else {
                        log.words.insert(word);
                    }
                }
            }
        }

        for w in &log.words {
            let word = unsafe { &**w };

            if self
                .per_culture
                .iter_mut()
                .all(|word_index| !word_index.contains_word(word))
            {
                self.word_intern.remove(word);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn word_intern(&self) -> &StrIntern {
        &self.word_intern
    }
}

unsafe impl Send for Index {}
unsafe impl Sync for Index {}

#[derive(Default)]
pub(crate) struct IndexLog {
    str: String,
    word: String,
    words: FxHashSet<*const str>,
}

unsafe impl Send for IndexLog {}
unsafe impl Sync for IndexLog {}

fn word_indexes(culture: Option<u8>, per_culture: &mut [WordIndex]) -> &mut [WordIndex] {
    match culture {
        Some(culture) => {
            let index = culture as usize;
            per_culture.get_mut(index..(index + 1)).unwrap_or_default()
        }
        None => &mut per_culture[..],
    }
}

fn ensure_size<T, F>(vec: &mut Vec<T>, index: usize, mut new: F)
where
    F: FnMut() -> T,
{
    let r = vec.len()..(index + 1);
    vec.extend(r.map(|_| new()));
}

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

fn contains<'a>(
    direction: Direction,
    word_index: &'a WordIndex,
    q: &WordQuery,
    out: &mut Vec<MatchEntry<'a>>,
) {
    let word = match direction {
        Direction::Forward => &q.word,
        Direction::Backward => q.backward_word(),
    };

    word_index.contains(word, out);
}

fn directional_word<'a>(word: &'a str, direction: Direction, temp_str: &'a mut String) -> &'a str {
    match direction {
        Direction::Backward => {
            temp_str.clear();
            temp_str.extend(word.chars().rev());
            &*temp_str
        }
        Direction::Forward => word,
    }
}

fn ends_with<'a>(
    direction: Direction,
    word_index: &'a WordIndex,
    q: &WordQuery,
    out: &mut Vec<MatchEntry<'a>>,
) {
    match direction {
        Direction::Forward => word_index.ends_with(&q.word, out),
        Direction::Backward => word_index.starts_with(q.backward_word(), out),
    }
}

fn fuzzy<'a>(
    direction: Direction,
    word_index: &'a WordIndex,
    q: &WordQuery,
    out: &mut Vec<MatchEntry<'a>>,
) {
    match direction {
        Direction::Backward => match q.backward_dfa() {
            Some(dfa) => word_index.fuzzy(dfa, q.backward_word().len(), out),
            None => word_index.ends_with(q.backward_word(), out),
        },
        Direction::Forward => match q.dfa() {
            Some(dfa) => word_index.fuzzy(dfa, q.word.len(), out),
            None => word_index.starts_with(&q.word, out),
        },
    }
}

fn insert_doc_word_list(
    word_indexes: &mut [WordIndex],
    attr_value: &str,
    doc_id: DocId,
    log: &mut IndexLog,
    word_intern: &mut StrIntern,
    direction: Direction,
) -> Vec<*const str> {
    let mut chars = attr_value.chars().peekable();
    let mut word_list = Vec::<*const str>::new();

    loop {
        find_next_word(&mut chars, &mut log.word);

        if log.word.is_empty() {
            break;
        }

        let mut word_index_iter = word_indexes.iter_mut();

        if let Some(word_index) = word_index_iter.next() {
            let word = directional_word(&log.word, direction, &mut log.str);

            let word =
                word_index.insert_word_doc(word, WordInternResolver::StrInter(word_intern), doc_id);

            for word_index in word_index_iter {
                word_index.insert_word_doc(word, WordInternResolver::StaticWord(word), doc_id);
            }

            word_list.push(word);
        }
    }

    word_list
}

fn starts_with<'a>(
    direction: Direction,
    word_index: &'a WordIndex,
    q: &WordQuery,
    out: &mut Vec<MatchEntry<'a>>,
) {
    match direction {
        Direction::Forward => word_index.starts_with(&q.word, out),
        Direction::Backward => word_index.ends_with(q.backward_word(), out),
    }
}

/*
#[cfg(test)]
mod tests {
    use std::hash::DefaultHasher;

    use indexmap::IndexMap;

    use super::*;
    use crate::{searcher::Attr, Presence};

    fn create_index(docs: &[&'static str], direction: Direction) -> Index {
        let mut index = Index::new(direction);
        let mut log = IndexLog::default();
        let mut attrs = AttrMap::default();

        attrs.insert("*".into(), Attr { culture: None, direction: Direction::Forward, priority: 0, index: 0 });

        docs.iter().enumerate().for_each(|(doc_id, doc)| {
            index.insert_doc_attribute(DocId(doc_id as u32), 0, doc, None, &mut log, &attrs);
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
            0,
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance(6), "seirtnuoc"),
                (MatchDistance(4), "yrtnuoc")
            ]
        );
    }

    #[test]
    fn contains_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("oun", WordQueryOp::Contains, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance(6), "countries"),
                (MatchDistance(4), "country")
            ]
        );
    }

    #[test]
    fn ends_with_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("try", WordQueryOp::EndsWith, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance(4), "yrtnuoc")]);
    }

    #[test]
    fn ends_with_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("try", WordQueryOp::EndsWith, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance(4), "country")]);
    }

    #[test]
    fn eq_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("country", WordQueryOp::Eq, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance(0), "yrtnuoc")]);
    }

    #[test]
    fn eq_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("country", WordQueryOp::Eq, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance(0), "country")]);
    }

    #[test]
    fn fuzzy_backward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Backward);

        index.query(
            &create_query("country", WordQueryOp::Fuzzy, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance(0), "yrtnuoc")]);
    }

    #[test]
    fn fuzzy_forward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("country", WordQueryOp::Fuzzy, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance(3), "countries"),
                (MatchDistance(0), "country")
            ]
        );
    }

    #[test]
    fn fuzzy_longer_query() {
        let mut matches = Vec::new();
        let index = create_index(&["country"], Direction::Forward);

        index.query(
            &create_query("countries", WordQueryOp::Fuzzy, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance(5), "country"),]);
    }

    #[test]
    fn fuzzy_3_letter_query() {
        let mut matches = Vec::new();
        let index = create_index(&["dmo"], Direction::Forward);

        index.query(
            &create_query("dmo", WordQueryOp::Fuzzy, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(matches, vec![(MatchDistance(0), "dmo"),]);
    }

    #[test]
    fn remove_first_doc() {
        let mut index = create_index(&["air canada", "air france"], Direction::Forward);
        let mut log = IndexLog::default();

        index.remove_doc(DocId(0), &mut log);

        assert_eq!(index.docs.len(), 1);

        assert_eq!(
            index.words.iter().map(|e| &*e.word).collect::<Vec<_>>(),
            vec!["air", "france"]
        );

        assert_eq!(
            index
                .words
                .iter()
                .fold(RoaringBitmap::new(), |acc, entry| acc | &entry.docs)
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
        let mut log = IndexLog::default();

        index.remove_doc(DocId(1), &mut log);

        assert_eq!(index.docs.len(), 1);

        assert_eq!(
            index.words.iter().map(|e| &*e.word).collect::<Vec<_>>(),
            vec!["air", "canada"]
        );

        assert_eq!(
            index
                .per_culture
                .iter()
                .fold(RoaringBitmap::new(), |acc, entry| acc | &entry.docs)
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
            0,
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance(5), "seirtnuoc"),
                (MatchDistance(3), "yrtnuoc")
            ]
        );
    }

    #[test]
    fn starts_with_foreward() {
        let mut matches = Vec::new();
        let index = create_index(&["country", "countries"], Direction::Forward);

        index.query(
            &create_query("coun", WordQueryOp::StartsWith, Presence::Optional),
            0,
            &mut matches,
        );

        assert_eq!(
            matches,
            vec![
                (MatchDistance(5), "countries"),
                (MatchDistance(3), "country")
            ]
        );
    }
}

 */
