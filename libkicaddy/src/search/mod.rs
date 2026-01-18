//! Full-text search for KiCAD symbol libraries using Tantivy

mod indexer;
mod schema;
mod searcher;

pub use indexer::{build_index, IndexStats};
pub use schema::index_path;
pub use searcher::{search, SearchOptions, SearchResult, SearchResults};

use thiserror::Error;

/// Errors that can occur during search operations
#[derive(Error, Debug)]
pub enum SearchError {
    #[error("Could not determine index path (set KICADDY_INDEX_PATH or ensure data directory exists)")]
    NoIndexPath,

    #[error("Search index not found. Run 'kicaddy index' to build it.")]
    IndexNotFound,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("Parse error: {0}")]
    ParseError(String),
}
