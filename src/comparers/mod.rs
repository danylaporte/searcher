mod match_distance_score;
mod proximity_seq_score;

use crate::{Direction, DocId, IndexResults, SearchResults};
use match_distance_score::MatchDistanceScore;
use proximity_seq_score::ProximitySeqScore;
use std::{
    cell::Cell,
    cmp::{min, Ordering},
    mem::swap,
};

pub fn compare(lid: DocId, lres: &SearchResults, rid: DocId, rres: &SearchResults) -> Ordering {
    thread_local! {
        static CELL: Cell<Comparer> = const { Cell::new(Comparer::new()) };
    }

    CELL.with(|cell| {
        let mut comparer = cell.take();
        let o = comparer.compare(lid, lres, rid, rres);

        cell.set(comparer);
        o
    })
}

struct Comparer {
    left: WorkingSet,
    right: WorkingSet,
    set: WorkingSet,
}

impl Comparer {
    const fn new() -> Self {
        Self {
            left: WorkingSet::new(),
            right: WorkingSet::new(),
            set: WorkingSet::new(),
        }
    }

    fn compare(
        &mut self,
        lid: DocId,
        lres: &SearchResults,
        rid: DocId,
        rres: &SearchResults,
    ) -> Ordering {
        let mut lside = Side::new(lid, lres, &mut self.left);
        let mut rside = Side::new(rid, rres, &mut self.right);
        let attr_priority_count = min(lside.attrs_priorities.len(), rside.attrs_priorities.len());

        let set = &mut self.set;

        for attr_priority_index in 0..attr_priority_count {
            let l = lside.match_distance(attr_priority_index, set);
            let r = rside.match_distance(attr_priority_index, set);
            let o = l.cmp(r);

            if o.is_ne() {
                return o;
            }

            let l = lside.proximity_seq(attr_priority_index, set);
            let r = rside.proximity_seq(attr_priority_index, set);
            let o = l.cmp(r);

            if o.is_ne() {
                return o;
            }
        }

        // This is important if we compare different searcher together, which may not have the same number of attributes.
        rside.attrs_priorities.len().cmp(&lside.attrs_priorities.len())
    }
}

impl Default for Comparer {
    fn default() -> Self {
        Self::new()
    }
}

struct WorkingSet {
    match_distance: MatchDistanceScore,
    proximity_seq: ProximitySeqScore,
}

impl WorkingSet {
    const fn new() -> Self {
        Self {
            match_distance: MatchDistanceScore::new(),
            proximity_seq: ProximitySeqScore::new(),
        }
    }

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
    doc_id: DocId,
    set: &'a mut WorkingSet,
    results: &'a SearchResults<'a>,
}

impl<'a> Side<'a> {
    fn new(doc_id: DocId, results: &'a SearchResults<'a>, set: &'a mut WorkingSet) -> Self {
        Self {
            attrs_priorities: results.searcher.attrs_priorities(),
            doc_id,
            results,
            set,
        }
    }

    fn match_distance<'b>(
        &'b mut self,
        attr_priority_index: usize,
        temp_set: &mut WorkingSet,
    ) -> &'b MatchDistanceScore {
        self.set.match_distance.clear();

        if let Some(a) = self.attrs_priorities.get(attr_priority_index) {
            for &(direction, attr_index) in a {
                let results = self.results.direction_index_results(direction);

                self.set
                    .match_distance(temp_set, self.doc_id, attr_index, results);
            }
        }

        &self.set.match_distance
    }

    fn proximity_seq<'b>(
        &'b mut self,
        attr_priority_index: usize,
        temp_set: &mut WorkingSet,
    ) -> &'b ProximitySeqScore {
        self.set.proximity_seq.clear();

        if let Some(a) = self.attrs_priorities.get(attr_priority_index) {
            for &(direction, attr_index) in a {
                let results = self.results.direction_index_results(direction);

                self.set
                    .proximity_seq(temp_set, self.doc_id, attr_index, results);
            }
        }

        &self.set.proximity_seq
    }
}
