use crate::IndexToQuery;
use std::cmp::{max, min, Ordering};

#[derive(Default)]
pub(super) struct ProximitySeqScore {
    count: usize,
    locations: Vec<Option<usize>>,
    proximity: usize,
    seq: usize,
}

impl ProximitySeqScore {
    fn add_word(&mut self, index: &IndexToQuery, word: *const str, word_location: usize) {
        if let Some(entry) = index.get(word) {
            let loc = match self.locations.get_mut(entry.query_index) {
                Some(l) => l,
                None => {
                    self.ensure_size(index.query_len());
                    self.locations.get_mut(entry.query_index).expect("index")
                }
            };

            let old = loc.replace(word_location);

            // if the count change, the proximity and sequence must be recomputed.
            if old.is_none() {
                self.count += 1;
                self.proximity = usize::MAX;
                self.seq = 0;
            }

            self.update_proximity_seq();
        }
    }

    pub(super) fn clear(&mut self) {
        if self.count > 0 {
            self.count = 0;
            self.locations.iter_mut().for_each(|o| *o = None);
            self.proximity = 0;
            self.seq = 0;
        }
    }

    fn ensure_size(&mut self, len: usize) {
        if self.locations.len() < len {
            let from = self.locations.len();
            let range = from..len;

            self.locations.extend(range.map(|_| None));
        }
    }

    pub(super) fn update(&mut self, index: &IndexToQuery, words: &[*const str]) {
        self.clear();

        for (word_location, &word) in words.iter().enumerate() {
            self.add_word(index, word, word_location);
        }
    }

    fn update_proximity_seq(&mut self) {
        if self.count > 1 {
            let (prox, seq) = self.locations.iter().filter_map(|l| *l).fold(
                (Proximity::new(), Seq::default()),
                |(mut prox, mut seq), index| {
                    prox.add(index);
                    seq.add(index);
                    (prox, seq)
                },
            );

            let prox = prox.value();

            // keep only the best proximity
            // and best seq for that proximity
            if self.proximity > prox {
                self.proximity = prox;
                self.seq = seq.seq;
            }
        }
    }
}

impl Eq for ProximitySeqScore {}

impl Ord for ProximitySeqScore {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut o = other.count.cmp(&self.count);

        if o.is_eq() {
            o = self.proximity.cmp(&other.proximity);

            if o.is_eq() {
                o = other.seq.cmp(&other.seq);
            }
        }

        o
    }
}

impl PartialEq for ProximitySeqScore {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count && self.proximity == other.proximity && self.seq == other.seq
    }
}

impl PartialOrd for ProximitySeqScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Proximity {
    max: usize,
    min: usize,
}

impl Proximity {
    fn new() -> Self {
        Self {
            max: 0,
            min: usize::MAX,
        }
    }

    fn add(&mut self, index: usize) {
        self.max = max(index, self.max);
        self.min = min(index, self.min);
    }

    fn value(self) -> usize {
        self.max - self.min
    }
}

#[derive(Default)]
struct Seq {
    last: Option<usize>,
    seq: usize,
}

impl Seq {
    fn add(&mut self, index: usize) {
        if let Some(last) = self.last {
            if last < index {
                self.seq += 1;
            }
        }

        self.last = Some(index);
    }
}
