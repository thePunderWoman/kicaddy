//! libkicaddy - Library for parsing and manipulating KiCAD files
//!
//! This library focuses on schematic editing and provides:
//! - KiCAD file parsing (schematics, symbols)
//! - Access to KiCAD symbol libraries
//! - Schematic manipulation APIs
//! - Command pattern for schematic operations

pub mod commands;
pub mod common;
pub mod config;
pub mod connectivity;
pub mod parser;
pub mod schematic;
pub mod search;
pub mod symbol;
pub mod tools;

// Re-export common types
pub use config::{ConfigError, KicadConfig};
pub use schematic::{parse_schematic, parse_schematic_str, Schematic, SchematicError};
pub use symbol::lookup::{find_symbol, LookupError};
pub use symbol::{parse_symbol_library, parse_symbol_library_str, Symbol, SymbolError, SymbolLibrary};
pub use search::{build_index, search, IndexStats, SearchError, SearchOptions, SearchResult, SearchResults};
pub use tools::{Tool, ToolError, ToolMetadata, ToolRegistry};
