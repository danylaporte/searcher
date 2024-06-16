use crate::{Direction, DocId, IndexResults, IndexToQuery, Searcher};
use roaring::RoaringBitmap;

pub struct SearchResults<'a> {
    backward: IndexResults<'a>,
    pub(crate) culture: u8,

    doc_ids: RoaringBitmap,

    forward: IndexResults<'a>,
    pub(crate) searcher: &'a Searcher,
}

impl<'a> SearchResults<'a> {
    pub(crate) fn new(
        backward: IndexResults<'a>,
        culture: u8,
        doc_ids: RoaringBitmap,
        forward: IndexResults<'a>,
        searcher: &'a Searcher,
    ) -> Self {
        Self {
            backward,
            culture,
            doc_ids,
            forward,
            searcher,
        }
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

    /// Gets matched words with distance for a document / attribute
    pub fn get_doc_attr_words_with_distance_and_query_index<'b>(
        &'b self,
        doc_id: DocId,
        attr: &str,
    ) -> impl Iterator<Item = (&'a str, Distance, QueryIndex)> + 'b {
        let (slice, index_query) = self.get_doc_attr_words_with_distance_impl(doc_id, attr);

        slice.iter().filter_map(|w| {
            let q = index_query.get(*w)?;
            Some((unsafe { &**w }, q.distance.0, q.query_index))
        })
    }

    /// Gets all attributes and words matched.
    pub fn get_doc_words_with_attr_distance_and_query_index<'b>(
        &'b self,
        doc_id: DocId,
    ) -> impl Iterator<Item = (Attr<'a>, Word<'a>, Distance, QueryIndex)> + 'b {
        self.searcher.attrs().iter().flat_map(move |a| {
            self.get_doc_attr_words_with_distance_and_query_index(doc_id, a.0)
                .map(|(word, distance, query_index)| (&**a.0, word, distance, query_index))
        })
    }

    fn get_doc_attr_words_with_distance_impl<'b>(
        &'b self,
        doc_id: DocId,
        attr: &str,
    ) -> (&'a [*const str], &'b IndexToQuery<'a>) {
        let Some(a) = self.searcher.attrs().get(attr) else {
            return (&[], &self.forward.index_to_query);
        };

        let results = match a.direction {
            Direction::Backward => &self.backward,
            Direction::Forward => &self.forward,
        };

        (
            results.index.get_doc_attribute_words(doc_id, a.index),
            &results.index_to_query,
        )
    }
}

type Attr<'a> = &'a str;
type Distance = u8;
type QueryIndex = usize;
type Word<'a> = &'a str;
