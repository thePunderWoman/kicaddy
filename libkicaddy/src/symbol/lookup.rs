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
    let filename = format!("{}.kicad_sym", library);

    // Search all library paths (official + 3rdparty)
    for base_path in config.all_symbol_lib_paths() {
        // First check directly in this path
        let path = base_path.join(&filename);
        if path.exists() {
            return Ok(path);
        }

        // For 3rdparty paths, search recursively in subdirectories
        if let Some(found) = find_library_recursive(&base_path, &filename) {
            return Ok(found);
        }
    }

    Err(LookupError::LibraryNotFound(library.to_string()))
}

/// Recursively search for a library file in a directory
fn find_library_recursive(dir: &PathBuf, filename: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Check directly in this subdirectory first
            let lib_path = path.join(filename);
            if lib_path.exists() {
                return Some(lib_path);
            }
            // Then recurse deeper
            if let Some(found) = find_library_recursive(&path, filename) {
                return Some(found);
            }
        }
    }

    None
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

    #[test]
    fn test_find_3rdparty_library() {
        if let Ok(config) = KicadConfig::detect() {
            // Try to find Espressif library (3rdparty, in subdirectory)
            let result = library_path(&config, "Espressif");
            if result.is_ok() {
                let path = result.unwrap();
                assert!(path.exists());
                assert!(path.to_string_lossy().contains("Espressif.kicad_sym"));
                // Verify we can parse a symbol from it
                let symbol_result = find_symbol(&config, "Espressif", "ESP32-C6-WROOM-1");
                assert!(symbol_result.is_ok(), "Should find ESP32-C6-WROOM-1 in Espressif library");
            }
            // If Espressif isn't installed, the test silently passes
        }
    }
}
