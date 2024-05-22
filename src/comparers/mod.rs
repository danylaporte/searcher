mod match_distance_score;
mod proximity_seq_score;

use crate::{Direction, DocId, IndexResults, SearchResults};
use match_distance_score::MatchDistanceScore;
use proximity_seq_score::ProximitySeqScore;
use std::{
    cmp::{min, Ordering},
    mem::swap,
};

pub struct Comparer<'a> {
    attr_priority_count: usize,
    left: Side<'a>,
    right: Side<'a>,
    set: WorkingSet,
}

impl<'a> Comparer<'a> {
    pub(crate) fn new(left: &'a SearchResults<'a>, right: &'a SearchResults<'a>) -> Self {
        Self {
            attr_priority_count: min(
                left.searcher.attrs_priorities().len(),
                right.searcher.attrs_priorities().len(),
            ),
            left: Side::new(left),
            right: Side::new(right),
            set: Default::default(),
        }
    }

    pub fn compare(&mut self, l: DocId, r: DocId) -> Ordering {
        let set = &mut self.set;

        for attr_priority_index in 0..self.attr_priority_count {
            let left = self.left.match_distance(attr_priority_index, l, set);
            let right = self.right.match_distance(attr_priority_index, r, set);
            let o = left.cmp(right);

            if o.is_ne() {
                return o;
            }

            let left = self.left.proximity_seq(attr_priority_index, l, set);
            let right = self.right.proximity_seq(attr_priority_index, r, set);
            let o = left.cmp(right);

            if o.is_ne() {
                return o;
            }
        }

        Ordering::Equal
    }
}

#[derive(Default)]
struct WorkingSet {
    match_distance: MatchDistanceScore,
    proximity_seq: ProximitySeqScore,
}

impl WorkingSet {
    fn match_distance(
        &mut self,
        set: &mut WorkingSet,
        id: DocId,
        attr_index: usize,
        results: &IndexResults,
    ) {
        let words = results.index.get_doc_attribute_words(id, attr_index);

        if !words.is_empty() {
            set.match_distance.update(&results.index_to_query, words);

            if self.match_distance > set.match_distance {
                swap(&mut self.match_distance, &mut set.match_distance);
            }
        }
    }

    fn proximity_seq(
        &mut self,
        set: &mut WorkingSet,
        id: DocId,
        attr_index: usize,
        results: &IndexResults,
    ) {
        let words = results.index.get_doc_attribute_words(id, attr_index);

        if !words.is_empty() {
            set.proximity_seq.update(&results.index_to_query, words);

            if self.proximity_seq > set.proximity_seq {
                swap(&mut self.proximity_seq, &mut set.proximity_seq);
            }
        }
    }
}

struct Side<'a> {
    attrs_priorities: &'a [Vec<(Direction, usize)>],
    set: WorkingSet,
    results: &'a SearchResults<'a>,
}

impl<'a> Side<'a> {
    fn new(results: &'a SearchResults<'a>) -> Self {
        Self {
            attrs_priorities: results.searcher.attrs_priorities(),
            results,
            set: Default::default(),
        }
    }

    fn match_distance<'b>(
        &'b mut self,
        attr_priority_index: usize,
        doc_id: DocId,
        temp_set: &mut WorkingSet,
    ) -> &'b MatchDistanceScore {
        self.set.match_distance.clear();

        if let Some(a) = self.attrs_priorities.get(attr_priority_index) {
            for &(direction, attr_index) in a {
                let results = self.results.direction_index_results(direction);

                self.set
                    .match_distance(temp_set, doc_id, attr_index, results);
            }
        }

        &self.set.match_distance
    }

    fn proximity_seq<'b>(
        &'b mut self,
        attr_priority_index: usize,
        doc_id: DocId,
        temp_set: &mut WorkingSet,
    ) -> &'b ProximitySeqScore {
        self.set.proximity_seq.clear();

        if let Some(a) = self.attrs_priorities.get(attr_priority_index) {
            for &(direction, attr_index) in a {
                let results = self.results.direction_index_results(direction);

                self.set
                    .proximity_seq(temp_set, doc_id, attr_index, results);
            }
        }

        &self.set.proximity_seq
    }
}
