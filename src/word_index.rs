use crate::{match_entry::MatchEntry, DocId, MatchDistance, StrIntern};
use levenshtein_automata::{Distance, DFA};
use roaring::RoaringBitmap;
use std::cmp::min;

pub(crate) struct WordIndex(Vec<WordIndexRow>);

impl WordIndex {
    pub(crate) const fn new() -> Self {
        Self(Vec::new())
    }

    fn binary_search(&self, word: &str) -> Result<usize, usize> {
        self.0.binary_search_by_key(&word, |t| t.word)
    }

    pub(crate) fn contains<'a>(&'a self, word: &str, out: &mut Vec<MatchEntry<'a>>) {
        out.extend(
            self.0
                .iter()
                .filter(|r| r.word.contains(word))
                .map(|r| r.match_entry_eq_distance(word)),
        );
    }

    pub(crate) fn contains_word(&self, word: &str) -> bool {
        self.binary_search(word).is_ok()
    }

    pub(crate) fn ends_with<'a>(&'a self, word: &str, out: &mut Vec<MatchEntry<'a>>) {
        out.extend(
            self.0
                .iter()
                .filter(|r| r.word.ends_with(word))
                .map(|r| r.match_entry_eq_distance(word)),
        );
    }

    pub(crate) fn fuzzy<'a>(&'a self, dfa: &DFA, word_len: usize, out: &mut Vec<MatchEntry<'a>>) {
        out.extend(self.0.iter().filter_map(|r| match dfa.eval(r.word) {
            Distance::AtLeast(_) => None,
            Distance::Exact(fuzzy_dist) => {
                let a = r.word.len();
                let word_dist = min(
                    a.saturating_sub(word_len) + word_len.saturating_sub(a),
                    0b111111,
                ) as u8;

                Some(MatchEntry {
                    distance: MatchDistance(fuzzy_dist + word_dist),
                    docs: &r.docs,
                    word: r.word,
                })
            }
        }));
    }

    pub(crate) fn eq<'a>(&'a self, word: &str, out: &mut Vec<MatchEntry<'a>>) {
        if let Ok(index) = self.binary_search(word) {
            out.push(unsafe { self.0.get_unchecked(index) }.match_entry_eq_distance(word));
        }
    }

    pub(crate) fn insert_word_doc(
        &mut self,
        word: &str,
        word_intern: WordInternResolver<'_>,
        doc_id: DocId,
    ) -> &'static str {
        let index = match self.binary_search(word) {
            Ok(index) => index,
            Err(index) => {
                let word = match word_intern {
                    WordInternResolver::StaticWord(word) => word,
                    WordInternResolver::StrInter(intern) => intern.insert(word),
                };

                self.0.insert(index, WordIndexRow::new(word));
                index
            }
        };

        let row = unsafe { self.0.get_unchecked_mut(index) };
        row.docs.insert(doc_id.0);
        row.word
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn remove_word_doc(&mut self, word: &str, doc_id: DocId) -> bool {
        match self.binary_search(word) {
            Ok(index) => {
                let row = unsafe { self.0.get_unchecked_mut(index) };
                row.docs.remove(doc_id.0);
                let is_empty = row.docs.is_empty();

                if is_empty {
                    self.0.remove(index);
                }

                is_empty
            }
            Err(_) => true,
        }
    }

    pub(crate) fn starts_with<'a>(&'a self, word: &str, out: &mut Vec<MatchEntry<'a>>) {
        let index = match self.binary_search(word) {
            Ok(index) => index,
            Err(index) => {
                if index >= self.0.len() {
                    return;
                }

                index
            }
        };

        out.extend(
            self.0[index..]
                .iter()
                .take_while(|r| r.word.starts_with(word))
                .map(|r| r.match_entry_eq_distance(word)),
        );
    }
}

struct WordIndexRow {
    docs: RoaringBitmap,

    /// InternStr
    word: &'static str,
}

impl WordIndexRow {
    fn new(word: &'static str) -> Self {
        Self {
            docs: RoaringBitmap::new(),
            word,
        }
    }

    fn match_entry_eq_distance<'a>(&'a self, word: &str) -> MatchEntry<'a> {
        let d = min(self.word.len() - word.len(), 255) as u8;

        MatchEntry {
            distance: MatchDistance(d),
            docs: &self.docs,
            word: self.word,
        }
    }
}

pub(crate) enum WordInternResolver<'a> {
    StaticWord(&'static str),
    StrInter(&'a mut StrIntern),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{str_intern::StrIntern, word_query::create_dfa};

    #[test]
    fn insert_remove_doc() {
        let mut word_index = WordIndex::new();
        let mut intern = StrIntern::new();

        let a = word_index.insert_word_doc(
            "w",
            WordInternResolver::StrInter(&mut intern),
            DocId::from(0),
        );
        let b = word_index.insert_word_doc(
            "w",
            WordInternResolver::StrInter(&mut intern),
            DocId::from(1),
        );

        assert!(std::ptr::addr_eq(a, b));
        assert_eq!(1, word_index.0.len());
        assert_eq!(2, word_index.0[0].docs.len());

        let can_delete = word_index.remove_word_doc(a, DocId::from(0));

        assert!(!can_delete);
        assert_eq!(1, word_index.0[0].docs.len());

        let can_delete = word_index.remove_word_doc(a, DocId::from(1));
        assert!(can_delete);

        assert!(word_index.0.is_empty());
    }

    #[test]
    fn query() {
        let mut word_index = WordIndex::new();
        let mut intern = StrIntern::new();

        word_index.insert_word_doc(
            "balance",
            WordInternResolver::StrInter(&mut intern),
            DocId::from(0),
        );
        word_index.insert_word_doc(
            "balle",
            WordInternResolver::StrInter(&mut intern),
            DocId::from(1),
        );

        let mut out = Vec::new();

        out.clear();
        word_index.contains("ll", &mut out);
        assert_eq!(out, vec![(MatchDistance(3), "balle")]);

        out.clear();
        word_index.ends_with("le", &mut out);
        assert_eq!(out, vec![(MatchDistance(3), "balle")]);

        out.clear();
        word_index.eq("balle", &mut out);
        assert_eq!(out, vec![(MatchDistance(0), "balle")]);

        out.clear();
        word_index.fuzzy(&create_dfa("bal").unwrap(), 3, &mut out);
        assert_eq!(
            out,
            vec![(MatchDistance(4), "balance"), (MatchDistance(2), "balle")]
        );

        out.clear();
        word_index.starts_with("bala", &mut out);
        assert_eq!(out, vec![(MatchDistance(3), "balance")]);
    }
}
