//! Search functionality for KiCAD symbols

use std::time::Instant;

use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, ReloadPolicy, Term};

use super::schema::{build_schema, index_path};
use super::SearchError;

/// A single search result
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub library: String,
    pub symbol: String,
    pub reference: String,
    pub description: String,
    pub keywords: String,
    pub score: f32,
    pub lib_id: String,
}

/// Search results with metadata
#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub results: Vec<SearchResult>,
    pub total_hits: usize,
    pub query_time_ms: u64,
}

/// Options for search queries
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub limit: usize,
    pub library_filter: Option<String>,
}

impl SearchOptions {
    pub fn new() -> Self {
        Self {
            limit: 10,
            library_filter: None,
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_library_filter(mut self, library: impl Into<String>) -> Self {
        self.library_filter = Some(library.into());
        self
    }
}

/// Search for symbols matching the query
pub fn search(query_str: &str, options: SearchOptions) -> Result<SearchResults, SearchError> {
    let start = Instant::now();

    // Open the index
    let idx_path = index_path().ok_or(SearchError::NoIndexPath)?;
    if !idx_path.exists() {
        return Err(SearchError::IndexNotFound);
    }

    let index = Index::open_in_dir(&idx_path)?;
    let (_schema, fields) = build_schema();
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    // Build query with boosts for different fields
    let terms: Vec<&str> = query_str.split_whitespace().collect();
    let mut subqueries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    for term in &terms {
        let term_lower = term.to_lowercase();

        // Create queries for each field with appropriate boosts
        let mut term_queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        // Name field (boost 3.0) - exact and fuzzy
        let name_term = Term::from_field_text(fields.name, &term_lower);
        term_queries.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(FuzzyTermQuery::new(name_term.clone(), 1, true)),
                3.0,
            )),
        ));

        // Keywords field (boost 2.0)
        let keywords_term = Term::from_field_text(fields.keywords, &term_lower);
        term_queries.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(FuzzyTermQuery::new(keywords_term.clone(), 1, true)),
                2.0,
            )),
        ));

        // Description field (boost 1.5)
        let desc_term = Term::from_field_text(fields.description, &term_lower);
        term_queries.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(FuzzyTermQuery::new(desc_term.clone(), 1, true)),
                1.5,
            )),
        ));

        // Pin names field (boost 0.5)
        let pin_term = Term::from_field_text(fields.pin_names, &term_lower);
        term_queries.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(FuzzyTermQuery::new(pin_term.clone(), 1, true)),
                0.5,
            )),
        ));

        // Combine term queries - at least one should match for this term
        let term_query = BooleanQuery::new(term_queries);
        subqueries.push((Occur::Must, Box::new(term_query)));
    }

    // Add library filter if specified
    if let Some(ref lib_filter) = options.library_filter {
        let lib_term = Term::from_field_text(fields.library, lib_filter);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(lib_term, IndexRecordOption::Basic)),
        ));
    }

    let query = BooleanQuery::new(subqueries);

    // Execute search
    let limit = if options.limit == 0 { 10 } else { options.limit };
    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

    // Collect results
    let mut results = Vec::new();
    for (score, doc_address) in &top_docs {
        let doc: tantivy::TantivyDocument = searcher.doc(*doc_address)?;

        let library = doc
            .get_first(fields.library)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let symbol = doc
            .get_first(fields.name)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let reference = doc
            .get_first(fields.reference)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let description = doc
            .get_first(fields.description)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let keywords = doc
            .get_first(fields.keywords)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let lib_id = format!("{}:{}", library, symbol);

        results.push(SearchResult {
            library,
            symbol,
            reference,
            description,
            keywords,
            score: *score,
            lib_id,
        });
    }

    let query_time_ms = start.elapsed().as_millis() as u64;
    let total_hits = top_docs.len();

    Ok(SearchResults {
        results,
        total_hits,
        query_time_ms,
    })
}
