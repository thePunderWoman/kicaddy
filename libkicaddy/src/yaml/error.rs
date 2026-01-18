//! Error types for YAML schematic processing

use std::fmt;

/// Errors that can occur during YAML schematic processing
#[derive(Debug, Clone)]
pub enum YamlError {
    /// Failed to parse YAML file
    ParseError(String),
    /// Invalid symbol format (not Library:Symbol)
    InvalidSymbolFormat { symbol: String, component: String },
    /// Invalid pin reference format (not REF:pin)
    InvalidPinReference { pin_ref: String, component: String },
    /// Duplicate component reference
    DuplicateReference(String),
    /// Connection has fewer than 2 endpoints
    InsufficientEndpoints { connection_index: usize },
    /// Component not found
    ComponentNotFound(String),
    /// Symbol not found in library
    SymbolNotFound { library: String, symbol: String },
    /// Library not found
    LibraryNotFound(String),
    /// Pin not found on component
    PinNotFound { reference: String, pin: String },
    /// Invalid position
    InvalidPosition { component: String, reason: String },
    /// Invalid angle value
    InvalidAngle { component: String, angle: f64 },
    /// Invalid mirror value
    InvalidMirror { component: String, value: String },
    /// KiCAD configuration error
    ConfigError(String),
    /// File I/O error
    IoError(String),
    /// Other errors
    Other(String),
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            YamlError::ParseError(msg) => write!(f, "YAML parse error: {}", msg),
            YamlError::InvalidSymbolFormat { symbol, component } => {
                write!(
                    f,
                    "Invalid symbol format '{}' for component '{}'. Expected 'Library:Symbol' format",
                    symbol, component
                )
            }
            YamlError::InvalidPinReference { pin_ref, component } => {
                write!(
                    f,
                    "Invalid pin reference '{}' in connection for '{}'. Expected 'REF:PIN' format",
                    pin_ref, component
                )
            }
            YamlError::DuplicateReference(reference) => {
                write!(f, "Duplicate component reference '{}'", reference)
            }
            YamlError::InsufficientEndpoints { connection_index } => {
                write!(
                    f,
                    "Connection {} has fewer than 2 endpoints",
                    connection_index
                )
            }
            YamlError::ComponentNotFound(reference) => {
                write!(f, "Component '{}' not found", reference)
            }
            YamlError::SymbolNotFound { library, symbol } => {
                write!(f, "Symbol '{}' not found in library '{}'", symbol, library)
            }
            YamlError::LibraryNotFound(library) => {
                write!(f, "Library '{}' not found", library)
            }
            YamlError::PinNotFound { reference, pin } => {
                write!(f, "Pin '{}' not found on component '{}'", pin, reference)
            }
            YamlError::InvalidPosition { component, reason } => {
                write!(f, "Invalid position for '{}': {}", component, reason)
            }
            YamlError::InvalidAngle { component, angle } => {
                write!(
                    f,
                    "Invalid angle {} for '{}'. Use 0, 90, 180, or 270",
                    angle, component
                )
            }
            YamlError::InvalidMirror { component, value } => {
                write!(
                    f,
                    "Invalid mirror value '{}' for '{}'. Use 'x', 'y', or omit",
                    value, component
                )
            }
            YamlError::ConfigError(msg) => write!(f, "KiCAD configuration error: {}", msg),
            YamlError::IoError(msg) => write!(f, "I/O error: {}", msg),
            YamlError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for YamlError {}

impl From<serde_yaml::Error> for YamlError {
    fn from(e: serde_yaml::Error) -> Self {
        YamlError::ParseError(e.to_string())
    }
}

impl From<std::io::Error> for YamlError {
    fn from(e: std::io::Error) -> Self {
        YamlError::IoError(e.to_string())
    }
}

impl From<crate::config::ConfigError> for YamlError {
    fn from(e: crate::config::ConfigError) -> Self {
        YamlError::ConfigError(e.to_string())
    }
}

impl From<crate::LookupError> for YamlError {
    fn from(e: crate::LookupError) -> Self {
        YamlError::Other(e.to_string())
    }
}

impl From<crate::commands::CommandError> for YamlError {
    fn from(e: crate::commands::CommandError) -> Self {
        YamlError::Other(e.to_string())
    }
}
