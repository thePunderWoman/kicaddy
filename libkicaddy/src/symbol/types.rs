//! Core data structures for KiCAD symbol libraries

use serde::{Deserialize, Serialize};

use super::graphics::GraphicItem;
use super::pin::Pin;

// Re-export common types for backwards compatibility
pub use crate::common::{Effects, Font, HorizontalJustify, Justify, Point, Position, Property, VerticalJustify};

/// A complete symbol library file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolLibrary {
    /// Format version (e.g., 20241209)
    pub version: u32,
    /// Generator that created the file
    pub generator: Option<String>,
    /// Generator version
    pub generator_version: Option<String>,
    /// Symbols in the library
    pub symbols: Vec<Symbol>,
}

/// A symbol (component) definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol name/identifier
    pub name: String,
    /// Parent symbol name for inheritance (via `extends` directive)
    pub extends: Option<String>,
    /// Whether to hide pin numbers
    pub pin_numbers_hide: bool,
    /// Pin name offset
    pub pin_names_offset: f64,
    /// Whether to hide pin names
    pub pin_names_hide: bool,
    /// Exclude from simulation
    pub exclude_from_sim: bool,
    /// Include in BOM
    pub in_bom: bool,
    /// Include on board
    pub on_board: bool,
    /// Symbol properties (Reference, Value, Footprint, etc.)
    pub properties: Vec<Property>,
    /// Child graphic units (named like "R_0_1")
    pub units: Vec<SymbolUnit>,
    /// Whether to embed fonts
    pub embedded_fonts: Option<bool>,
}

impl Symbol {
    /// Get a property by name
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name == name)
    }

    /// Get the Reference property value
    pub fn reference(&self) -> Option<&str> {
        self.property("Reference").map(|p| p.value.as_str())
    }

    /// Get the Value property value
    pub fn value(&self) -> Option<&str> {
        self.property("Value").map(|p| p.value.as_str())
    }

    /// Get the Description property value
    pub fn description(&self) -> Option<&str> {
        self.property("Description").map(|p| p.value.as_str())
    }

    /// Get the Footprint property value
    pub fn footprint(&self) -> Option<&str> {
        self.property("Footprint").map(|p| p.value.as_str())
    }

    /// Get the ki_keywords property value
    pub fn keywords(&self) -> Option<&str> {
        self.property("ki_keywords").map(|p| p.value.as_str())
    }

    /// Get all pins across all units
    pub fn pins(&self) -> impl Iterator<Item = &Pin> {
        self.units.iter().flat_map(|u| u.pins.iter())
    }
}

/// A symbol unit (graphical representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolUnit {
    /// Unit name (e.g., "R_0_1" for symbol "R", unit 0, style 1)
    pub name: String,
    /// Graphic items (rectangles, lines, etc.)
    pub graphics: Vec<GraphicItem>,
    /// Pins in this unit
    pub pins: Vec<Pin>,
}
