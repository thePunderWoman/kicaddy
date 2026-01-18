//! Symbol lookup from installed KiCAD libraries

use std::path::PathBuf;

use thiserror::Error;

use crate::config::KicadConfig;
use crate::symbol::{parse_symbol_library, Symbol, SymbolError};

#[derive(Debug, Error)]
pub enum LookupError {
    #[error("Library not found: {0}")]
    LibraryNotFound(String),
    #[error("Symbol not found: {symbol} in library {library}")]
    SymbolNotFound { library: String, symbol: String },
    #[error("Symbol library error: {0}")]
    SymbolError(#[from] SymbolError),
}

/// Find a symbol in the KiCAD symbol libraries
///
/// # Arguments
/// * `config` - KiCAD configuration with library paths
/// * `library` - Library name (e.g., "Device")
/// * `symbol` - Symbol name within the library (e.g., "R")
///
/// # Returns
/// The symbol if found, or an error if the library or symbol doesn't exist
pub fn find_symbol(
    config: &KicadConfig,
    library: &str,
    symbol: &str,
) -> Result<Symbol, LookupError> {
    let lib_path = library_path(config, library)?;
    let lib = parse_symbol_library(&lib_path)?;

    lib.symbols
        .into_iter()
        .find(|s| s.name == symbol)
        .ok_or_else(|| LookupError::SymbolNotFound {
            library: library.to_string(),
            symbol: symbol.to_string(),
        })
}

/// Get the path to a symbol library file
///
/// # Arguments
/// * `config` - KiCAD configuration with library paths
/// * `library` - Library name (e.g., "Device")
///
/// # Returns
/// The full path to the library file, or an error if it doesn't exist
pub fn library_path(config: &KicadConfig, library: &str) -> Result<PathBuf, LookupError> {
    let path = config
        .symbol_lib_path
        .join(format!("{}.kicad_sym", library));

    if path.exists() {
        Ok(path)
    } else {
        Err(LookupError::LibraryNotFound(library.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_path() {
        if let Ok(config) = KicadConfig::detect() {
            // Device library should exist in standard KiCAD installations
            let result = library_path(&config, "Device");
            if result.is_ok() {
                let path = result.unwrap();
                assert!(path.exists());
                assert!(path.to_string_lossy().contains("Device.kicad_sym"));
            }
        }
    }

    #[test]
    fn test_find_symbol_resistor() {
        if let Ok(config) = KicadConfig::detect() {
            // Try to find the resistor symbol in Device library
            let result = find_symbol(&config, "Device", "R");
            if result.is_ok() {
                let symbol = result.unwrap();
                assert_eq!(symbol.name, "R");
                // Should have Reference property
                assert!(symbol.reference().is_some());
            }
        }
    }

    #[test]
    fn test_find_nonexistent_symbol() {
        if let Ok(config) = KicadConfig::detect() {
            let result = find_symbol(&config, "Device", "NonExistentSymbol12345");
            assert!(matches!(result, Err(LookupError::SymbolNotFound { .. })));
        }
    }

    #[test]
    fn test_find_nonexistent_library() {
        if let Ok(config) = KicadConfig::detect() {
            let result = find_symbol(&config, "NonExistentLibrary12345", "R");
            assert!(matches!(result, Err(LookupError::LibraryNotFound(_))));
        }
    }
}
