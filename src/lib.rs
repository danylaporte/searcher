mod attr_props;
mod comparers;
mod direction;
mod doc_id;
mod index;
//mod index_old;
mod index_results;
mod index_to_query;
mod match_distance;
mod match_entry;
mod min_match_level;
mod presence;
mod search_query;
mod search_results;
mod searcher;
mod str_intern;
mod word_index;
mod word_query;
mod word_query_op;

pub use attr_props::AttrProps;
pub use comparers::{compare, Comparer};
pub use direction::Direction;
pub use doc_id::DocId;
use index::{Index, IndexLog};
use index_results::IndexResults;
use index_to_query::IndexToQuery;
use match_distance::MatchDistance;
use match_entry::MatchEntry;
pub use min_match_level::MinMatchLevel;
use presence::Presence;
pub use search_query::SearchQuery;
pub use search_results::SearchResults;
use searcher::AttrMap;
pub use searcher::Searcher;
use str_intern::StrIntern;
use word_index::{WordIndex, WordInternResolver};
use word_query::WordQuery;
use word_query_op::WordQueryOp;
