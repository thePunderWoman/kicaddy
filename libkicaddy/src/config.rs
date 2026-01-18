//! Configuration for KiCAD paths and settings

use std::path::PathBuf;
use thiserror::Error;

/// Environment variable for KiCAD installation path
pub const KICAD_PATH_ENV: &str = "KICAD_PATH";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("KiCAD installation not found. Set {KICAD_PATH_ENV} environment variable or install KiCAD to a standard location.")]
    KicadNotFound,
    #[error("Symbol library path not found: {0}")]
    SymbolLibraryNotFound(PathBuf),
}

/// Configuration for KiCAD installation and paths
#[derive(Debug, Clone)]
pub struct KicadConfig {
    /// Root path of the KiCAD installation
    pub kicad_path: PathBuf,
    /// Path to symbol libraries
    pub symbol_lib_path: PathBuf,
    /// Path to footprint libraries
    pub footprint_lib_path: PathBuf,
}

impl KicadConfig {
    /// Create configuration from environment variable or detect standard installation
    pub fn detect() -> Result<Self, ConfigError> {
        let kicad_path = Self::find_kicad_path()?;
        Self::from_path(kicad_path)
    }

    /// Get all symbol library paths to index (official + 3rdparty/plugins)
    pub fn all_symbol_lib_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.symbol_lib_path.clone()];

        // Add 3rdparty/plugin symbol paths
        for path in Self::third_party_symbol_paths() {
            if path.exists() {
                paths.push(path);
            }
        }

        paths
    }

    /// Get 3rdparty/plugin symbol library paths
    fn third_party_symbol_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(home) = dirs::home_dir() {
            // KiCAD stores plugins in ~/Documents/KiCad/<version>/3rdparty/symbols
            let kicad_docs = home.join("Documents").join("KiCad");
            if kicad_docs.exists() {
                // Scan for version directories (e.g., 8.0, 9.0)
                if let Ok(entries) = std::fs::read_dir(&kicad_docs) {
                    for entry in entries.flatten() {
                        let symbols_path = entry.path().join("3rdparty").join("symbols");
                        if symbols_path.exists() {
                            paths.push(symbols_path);
                        }
                    }
                }
            }
        }

        paths
    }

    /// Create configuration from a specific KiCAD path
    pub fn from_path(kicad_path: PathBuf) -> Result<Self, ConfigError> {
        let symbol_lib_path = kicad_path.join("symbols");
        let footprint_lib_path = kicad_path.join("footprints");

        Ok(Self {
            kicad_path,
            symbol_lib_path,
            footprint_lib_path,
        })
    }

    /// Find KiCAD installation path from environment or standard locations
    fn find_kicad_path() -> Result<PathBuf, ConfigError> {
        // First, check environment variable
        if let Ok(path) = std::env::var(KICAD_PATH_ENV) {
            let path = PathBuf::from(path);
            if path.exists() {
                return Ok(path);
            }
        }

        // Check standard installation locations
        let standard_paths = Self::standard_paths();
        for path in standard_paths {
            if path.exists() {
                return Ok(path);
            }
        }

        Err(ConfigError::KicadNotFound)
    }

    /// Get standard KiCAD installation paths for the current platform
    fn standard_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "macos")]
        {
            // KiCAD 8.x on macOS
            paths.push(PathBuf::from(
                "/Applications/KiCad/KiCad.app/Contents/SharedSupport",
            ));
            // Homebrew installation
            paths.push(PathBuf::from("/opt/homebrew/share/kicad"));
            paths.push(PathBuf::from("/usr/local/share/kicad"));
        }

        #[cfg(target_os = "linux")]
        {
            paths.push(PathBuf::from("/usr/share/kicad"));
            paths.push(PathBuf::from("/usr/local/share/kicad"));
        }

        #[cfg(target_os = "windows")]
        {
            paths.push(PathBuf::from("C:\\Program Files\\KiCad\\share\\kicad"));
            paths.push(PathBuf::from(
                "C:\\Program Files (x86)\\KiCad\\share\\kicad",
            ));
        }

        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_paths_not_empty() {
        let paths = KicadConfig::standard_paths();
        assert!(!paths.is_empty());
    }
}
