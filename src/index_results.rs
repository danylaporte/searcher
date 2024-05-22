use crate::{Index, IndexToQuery};

pub(crate) struct IndexResults<'a> {
    pub index: &'a Index,
    pub index_to_query: IndexToQuery<'a>,
}
