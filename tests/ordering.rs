use searcher::{compare, DocId, SearchQuery, Searcher};
use std::cmp::Ordering;

fn searcher(docs: &[&'static str]) -> Searcher {
    let mut out = Searcher::new();

    out.set_attribute("*".into(), Default::default());

    docs.iter().enumerate().for_each(|(doc_id, doc)| {
        out.insert_doc_attribute(DocId::from(doc_id as u32), "*", doc);
    });

    out
}

#[test]
fn single_word() {
    let searcher = searcher(&["country", "count"]);
    let results = searcher.query(&SearchQuery::new("count"));

    assert_eq!(
        compare(DocId::from(0), &results, DocId::from(1), &results),
        Ordering::Greater
    );
}

#[test]
fn multiple_word() {
    let searcher = searcher(&["count topic", "count"]);
    let results = searcher.query(&SearchQuery::new("count topic"));

    assert_eq!(
        compare(DocId::from(0), &results, DocId::from(1), &results),
        Ordering::Less
    );
}

#[test]
fn one_vs_multiple_word1() {
    let searcher = searcher(&["encours", "en cours"]);
    let results = searcher.query(&SearchQuery::new("encours"));

    assert_eq!(
        compare(DocId::from(0), &results, DocId::from(1), &results),
        Ordering::Less
    );
}

#[test]
fn one_vs_multiple_word2() {
    let searcher = searcher(&["encours", "en cours"]);
    let results = searcher.query(&SearchQuery::new("en cours"));

    assert_eq!(
        compare(DocId::from(0), &results, DocId::from(1), &results),
        Ordering::Greater
    );
}
