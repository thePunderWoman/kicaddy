//! Index building for KiCAD symbol search

use std::fs;
use std::path::Path;
use std::time::Instant;

use tantivy::{Index, IndexWriter};

use crate::config::KicadConfig;
use crate::symbol::parse_symbol_library;

use super::schema::{build_schema, index_path};
use super::SearchError;

/// Statistics about the indexing operation
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub libraries_indexed: usize,
    pub symbols_indexed: usize,
    pub libraries_failed: usize,
    pub elapsed_ms: u64,
}

/// Build or rebuild the search index from KiCAD symbol libraries
pub fn build_index(config: &KicadConfig) -> Result<IndexStats, SearchError> {
    let start = Instant::now();

    // Get index path
    let idx_path = index_path().ok_or(SearchError::NoIndexPath)?;

    // Create index directory if needed
    if idx_path.exists() {
        fs::remove_dir_all(&idx_path)?;
    }
    fs::create_dir_all(&idx_path)?;

    // Build schema and create index
    let (schema, fields) = build_schema();
    let index = Index::create_in_dir(&idx_path, schema)?;

    // Create writer with reasonable memory budget
    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let mut libraries_indexed = 0;
    let mut symbols_indexed = 0;
    let mut libraries_failed = 0;

    // Read all .kicad_sym files from all symbol library paths (recursively)
    for lib_path in config.all_symbol_lib_paths() {
        index_directory(
            &mut writer,
            &fields,
            &lib_path,
            &mut libraries_indexed,
            &mut symbols_indexed,
            &mut libraries_failed,
        );
    }

    // Commit all changes
    writer.commit()?;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    Ok(IndexStats {
        libraries_indexed,
        symbols_indexed,
        libraries_failed,
        elapsed_ms,
    })
}

/// Recursively index a directory for .kicad_sym files
fn index_directory(
    writer: &mut IndexWriter,
    fields: &super::schema::SearchFields,
    dir: &Path,
    libraries_indexed: &mut usize,
    symbols_indexed: &mut usize,
    libraries_failed: &mut usize,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Recurse into subdirectories
            index_directory(writer, fields, &path, libraries_indexed, symbols_indexed, libraries_failed);
        } else if path.extension().map(|ext| ext == "kicad_sym").unwrap_or(false) {
            match index_library(writer, fields, &path) {
                Ok(count) => {
                    *libraries_indexed += 1;
                    *symbols_indexed += count;
                }
                Err(_) => {
                    *libraries_failed += 1;
                }
            }
        }
    }
}

/// Index a single symbol library file
fn index_library(
    writer: &mut IndexWriter,
    fields: &super::schema::SearchFields,
    path: &Path,
) -> Result<usize, SearchError> {
    let library_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let lib = parse_symbol_library(path).map_err(|e| SearchError::ParseError(e.to_string()))?;

    let mut count = 0;
    for symbol in &lib.symbols {
        // Get symbol data
        let reference = symbol.reference().unwrap_or("");
        let description = symbol.description().unwrap_or("");
        let keywords = symbol.keywords().unwrap_or("");

        // Collect pin names
        let pin_names: Vec<&str> = symbol
            .pins()
            .filter_map(|p| {
                let name = p.name.name.as_str();
                // Skip generic pin names
                if name.is_empty() || name == "~" {
                    None
                } else {
                    Some(name)
                }
            })
            .collect();
        let pin_names_str = pin_names.join(" ");

        // Create document
        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(fields.library, library_name);
        doc.add_text(fields.name, &symbol.name);
        doc.add_text(fields.reference, reference);
        doc.add_text(fields.description, description);
        doc.add_text(fields.keywords, keywords);
        doc.add_text(fields.pin_names, &pin_names_str);

        writer.add_document(doc)?;
        count += 1;
    }

    Ok(count)
}
