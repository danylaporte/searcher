use crate::{Index, IndexToQuery};

pub(crate) struct IndexResults<'a> {
    pub(crate) index: &'a Index,
    pub(crate) index_to_query: IndexToQuery<'a>,
}

unsafe impl<'a> Send for IndexResults<'a> {}
unsafe impl<'a> Sync for IndexResults<'a> {}
