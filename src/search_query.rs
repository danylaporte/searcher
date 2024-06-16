use crate::{presence::Presence, word_query_op::WordQueryOp, MinMatchLevel, WordQuery};
use std::{iter::Peekable, str::Chars};
use str_utils::char_map::lower_no_accent_char;

pub struct SearchQuery {
    pub(crate) culture: u8,
    pub(crate) words: Vec<WordQuery>,
}

impl SearchQuery {
    pub fn new(culture: u8, s: &str) -> Self {
        let mut chars = s.chars().peekable();
        let mut words = Vec::new();

        while let Some(token) = parse_token(&mut chars, words.len()) {
            if !token.word.is_empty() {
                words.push(token);
            }
        }

        Self { culture, words }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Force the minimal level of matching.
    pub fn set_min_match_level(&mut self, level: MinMatchLevel) {
        match level {
            MinMatchLevel::Contains => {
                self.words.iter_mut().for_each(|w| {
                    if matches!(w.op, WordQueryOp::Fuzzy) {
                        w.op = WordQueryOp::Contains;
                    }
                });
            }
            MinMatchLevel::Equal => self.words.iter_mut().for_each(|w| w.op = WordQueryOp::Eq),
            MinMatchLevel::Fuzzy => {}
        }
    }
}

fn parse_token(chars: &mut Peekable<Chars>, index: usize) -> Option<WordQuery> {
    let mut presence = Presence::Optional;
    let mut op = WordQueryOp::Fuzzy;
    let mut text = String::new();

    loop {
        match chars.next()? {
            '+' => presence = Presence::Required,
            '-' => presence = Presence::Denied,
            '*' => op = WordQueryOp::EndsWith,
            c if c == '\'' || c == '"' => {
                take_until(chars, &mut text, |v| v == c);
                chars.next(); // eat the last quote.
                op = WordQueryOp::Eq;
                break;
            }
            c if c.is_alphanumeric() => {
                lower_no_accent_char(c).for_each(|c| text.push(c));
                take_until(chars, &mut text, |c| !c.is_alphanumeric());
                break;
            }
            c if c.is_whitespace() => {
                presence = Presence::Optional;
                op = WordQueryOp::Fuzzy;
            }
            _ => {}
        }
    }

    loop {
        match chars.peek() {
            Some('*') => {
                chars.next();

                if chars.peek().map_or(true, |c| c.is_whitespace()) {
                    match op {
                        WordQueryOp::Fuzzy => op = WordQueryOp::StartsWith,
                        WordQueryOp::Contains | WordQueryOp::Eq | WordQueryOp::StartsWith => {}
                        WordQueryOp::EndsWith => op = WordQueryOp::Contains,
                    }
                }

                continue;
            }

            Some(c) => {
                if c.is_alphanumeric() || c.is_whitespace() {
                    break;
                }
            }

            None => break,
        }
        chars.next();
    }

    Some(WordQuery::new(text.into_boxed_str(), op, presence, index))
}

fn take_until<F>(chars: &mut Peekable<Chars>, s: &mut String, f: F)
where
    F: Fn(char) -> bool,
{
    while chars.peek().map_or(false, |c| !f(*c)) {
        #[allow(clippy::unwrap_used)]
        let c = chars.next().unwrap();

        if c.is_alphanumeric() {
            lower_no_accent_char(c).for_each(|c| s.push(c));
        } else if s.chars().last().map_or(false, |c| !c.is_whitespace()) {
            s.push(' ');
        }
    }
}

#[test]
fn single_word() {
    assert_eq!(
        SearchQuery::new(0, "start*").words,
        vec![("start", WordQueryOp::StartsWith)]
    );
    assert_eq!(
        SearchQuery::new(0, "*end").words,
        vec![("end", WordQueryOp::EndsWith)]
    );
    assert_eq!(
        SearchQuery::new(0, "*contains*").words,
        vec![("contains", WordQueryOp::Contains)]
    );
    assert_eq!(
        SearchQuery::new(0, "fuzzy").words,
        vec![("fuzzy", WordQueryOp::Fuzzy)]
    );
}

#[test]
fn multiple_words() {
    assert_eq!(
        SearchQuery::new(0, "starT* *eNd *Contains* Fuzzy").words,
        vec![
            ("start", WordQueryOp::StartsWith),
            ("end", WordQueryOp::EndsWith),
            ("contains", WordQueryOp::Contains),
            ("fuzzy", WordQueryOp::Fuzzy)
        ]
    );
}
