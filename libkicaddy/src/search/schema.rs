//! Tantivy schema definition for KiCAD symbol search

use std::path::PathBuf;

use tantivy::schema::{Field, Schema, STORED, STRING, TextFieldIndexing, TextOptions, IndexRecordOption};

/// Fields in the search index
pub struct SearchFields {
    pub library: Field,
    pub name: Field,
    pub reference: Field,
    pub description: Field,
    pub keywords: Field,
    pub pin_names: Field,
}

/// Build the tantivy schema for symbol search
pub fn build_schema() -> (Schema, SearchFields) {
    let mut schema_builder = Schema::builder();

    // Library name - stored for filtering and display, not tokenized
    let library = schema_builder.add_text_field("library", STRING | STORED);

    // Symbol name - tokenized with boost for matching
    let name_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let name = schema_builder.add_text_field("name", name_options);

    // Reference designator (R, C, U, etc.) - stored but not tokenized
    let reference = schema_builder.add_text_field("reference", STRING | STORED);

    // Description - tokenized with moderate boost
    let description_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let description = schema_builder.add_text_field("description", description_options);

    // Keywords (ki_keywords) - tokenized with high boost
    let keywords_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let keywords = schema_builder.add_text_field("keywords", keywords_options);

    // Pin names - tokenized but not stored (for interface searches like "SDA SCL")
    let pin_names_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
    let pin_names = schema_builder.add_text_field("pin_names", pin_names_options);

    let schema = schema_builder.build();
    let fields = SearchFields {
        library,
        name,
        reference,
        description,
        keywords,
        pin_names,
    };

    (schema, fields)
}

/// Get the index directory path
///
/// Priority:
/// 1. `$KICADDY_INDEX_PATH` environment variable
/// 2. XDG data directory:
///    - macOS: `~/Library/Application Support/kicaddy/index`
///    - Linux: `~/.local/share/kicaddy/index`
pub fn index_path() -> Option<PathBuf> {
    // Check environment variable first
    if let Ok(path) = std::env::var("KICADDY_INDEX_PATH") {
        return Some(PathBuf::from(path));
    }

    // Use XDG data directory
    dirs::data_dir().map(|d| d.join("kicaddy").join("index"))
}
