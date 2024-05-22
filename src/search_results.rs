use crate::{Comparer, Direction, DocId, IndexResults, Searcher};
use roaring::RoaringBitmap;

pub struct SearchResults<'a> {
    backward: IndexResults<'a>,
    doc_ids: RoaringBitmap,
    forward: IndexResults<'a>,
    pub(crate) searcher: &'a Searcher,
}

impl<'a> SearchResults<'a> {
    pub(crate) fn new(
        backward: IndexResults<'a>,
        doc_ids: RoaringBitmap,
        forward: IndexResults<'a>,
        searcher: &'a Searcher,
    ) -> Self {
        Self {
            backward,
            doc_ids,
            forward,
            searcher,
        }
    }

    pub fn comparer(&self) -> Comparer {
        Comparer::new(self, self)
    }

    pub fn contains_doc_id(&self, id: DocId) -> bool {
        self.doc_ids.contains(id.0)
    }

    pub(crate) fn direction_index_results(&self, direction: Direction) -> &IndexResults<'a> {
        match direction {
            Direction::Forward => &self.forward,
            Direction::Backward => &self.backward,
        }
    }
}
