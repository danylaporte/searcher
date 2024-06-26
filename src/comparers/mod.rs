mod match_distance_score;
mod proximity_seq_score;

use crate::{Direction, DocId, IndexResults, SearchResults};
use match_distance_score::MatchDistanceScore;
use proximity_seq_score::ProximitySeqScore;
use std::{
    cell::Cell,
    cmp::Ordering,
    fmt::{self, Debug, Formatter},
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

#[derive(Debug)]
pub struct Comparer {
    left: WorkingSet,
    right: WorkingSet,
    set: WorkingSet,
}

impl Comparer {
    pub const fn new() -> Self {
        Self {
            left: WorkingSet::new(),
            right: WorkingSet::new(),
            set: WorkingSet::new(),
        }
    }

    pub fn compare(
        &mut self,
        lid: DocId,
        lres: &SearchResults,
        rid: DocId,
        rres: &SearchResults,
    ) -> Ordering {
        let mut lside = Side::new(lid, lres, &mut self.left);
        let mut rside = Side::new(rid, rres, &mut self.right);

        let set = &mut self.set;

        let mut l = lside.attrs_priorities.iter();
        let mut r = rside.attrs_priorities.iter();

        loop {
            match (l.next(), r.next()) {
                (Some((_l_priority, l_attrs)), Some((_r_priority, r_attrs))) => {
                    let l = lside.match_distance(l_attrs, set);
                    let r = rside.match_distance(r_attrs, set);
                    let o = l.cmp(r);

                    if o.is_ne() {
                        return o;
                    }

                    let l = lside.proximity_seq(l_attrs, set);
                    let r = rside.proximity_seq(r_attrs, set);
                    let o = l.cmp(r);

                    if o.is_ne() {
                        return o;
                    }
                }

                // Reverse Option here.
                (None, Some(_)) => return Ordering::Greater,
                (Some(_), None) => return Ordering::Less,
                (None, None) => return Ordering::Equal,
            }
        }
    }
}

impl Default for Comparer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
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
    attrs_priorities: &'a [(u8, Vec<(Direction, usize)>)],
    doc_id: DocId,
    set: &'a mut WorkingSet,
    results: &'a SearchResults<'a>,
}

impl<'a> Side<'a> {
    fn new(doc_id: DocId, results: &'a SearchResults<'a>, set: &'a mut WorkingSet) -> Self {
        Self {
            attrs_priorities: results.searcher.attrs_priorities(results.culture),
            doc_id,
            results,
            set,
        }
    }

    fn match_distance<'b>(
        &'b mut self,
        attrs: &[(Direction, usize)],
        temp_set: &mut WorkingSet,
    ) -> &'b MatchDistanceScore {
        self.set.match_distance.clear();

        for &(direction, attr_index) in attrs {
            let results = self.results.direction_index_results(direction);

            self.set
                .match_distance(temp_set, self.doc_id, attr_index, results);
        }

        &self.set.match_distance
    }

    fn proximity_seq<'b>(
        &'b mut self,
        attrs: &[(Direction, usize)],
        temp_set: &mut WorkingSet,
    ) -> &'b ProximitySeqScore {
        self.set.proximity_seq.clear();

        for &(direction, attr_index) in attrs {
            let results = self.results.direction_index_results(direction);

            self.set
                .proximity_seq(temp_set, self.doc_id, attr_index, results);
        }

        &self.set.proximity_seq
    }
}

impl<'a> Debug for Side<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Side")
            .field("attrs_priorities", &self.attrs_priorities)
            .field("doc_id", &self.doc_id)
            .field("set", &self.set)
            .finish()
    }
}
