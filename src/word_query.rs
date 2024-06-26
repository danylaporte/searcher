use crate::{presence::Presence, Direction, WordQueryOp};
use levenshtein_automata::{LevenshteinAutomatonBuilder, DFA};
use once_cell::sync::OnceCell;
use std::fmt::{self, Debug, Formatter};

pub(crate) struct WordQuery {
    backward_dfa: OnceCell<Option<DFA>>,
    backward_word: OnceCell<Box<str>>,
    dfa: OnceCell<Option<DFA>>,

    /// chars len
    pub index: usize,
    pub op: WordQueryOp,
    pub presence: Presence,
    pub word: Box<str>,
}

impl WordQuery {
    pub(crate) fn new(word: Box<str>, op: WordQueryOp, presence: Presence, index: usize) -> Self {
        Self {
            backward_dfa: OnceCell::new(),
            backward_word: OnceCell::new(),
            dfa: OnceCell::new(),
            index,
            op,
            presence,
            word,
        }
    }

    pub(crate) fn backward_dfa(&self) -> Option<&DFA> {
        init_dfa(&self.backward_dfa, self.backward_word())
    }

    pub(crate) fn backward_word(&self) -> &str {
        self.backward_word
            .get_or_init(|| self.word.chars().rev().collect::<String>().into_boxed_str())
    }

    pub(crate) fn dfa(&self) -> Option<&DFA> {
        init_dfa(&self.dfa, &self.word)
    }

    pub(crate) fn directional_word(&self, direction: Direction) -> &str {
        match direction {
            Direction::Forward => &self.word,
            Direction::Backward => self.backward_word(),
        }
    }
}

impl Debug for WordQuery {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("WordQuery")
            .field("op", &self.op)
            .field("presence", &self.presence)
            .field("word", &self.word)
            .finish()
    }
}

#[cfg(test)]
impl PartialEq<(&str, WordQueryOp)> for WordQuery {
    fn eq(&self, other: &(&str, WordQueryOp)) -> bool {
        &*self.word == other.0 && self.op == other.1
    }
}

pub(crate) fn create_dfa(word: &str) -> Option<DFA> {
    match word.chars().count() {
        0..=2 => None,
        3..=5 => Some(LevenshteinAutomatonBuilder::new(1, true).build_prefix_dfa(word)),
        6..=8 => Some(LevenshteinAutomatonBuilder::new(2, true).build_prefix_dfa(word)),
        9.. => Some(LevenshteinAutomatonBuilder::new(3, true).build_prefix_dfa(word)),
    }
}

fn init_dfa<'a>(dfa: &'a OnceCell<Option<DFA>>, word: &str) -> Option<&'a DFA> {
    dfa.get_or_init(|| create_dfa(word)).as_ref()
}
