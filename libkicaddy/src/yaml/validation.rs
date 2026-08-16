//! Validation for YAML schematic definitions

use std::collections::{HashMap, HashSet};

use super::error::YamlError;
use super::types::YamlSchematic;
use crate::config::KicadConfig;
use crate::schematic::pin_matches;
use crate::symbol::lookup::find_symbol;
use crate::symbol::Symbol;

/// Validation result with warnings and errors
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// List of errors (compilation will fail)
    pub errors: Vec<YamlError>,
    /// List of warnings (compilation will proceed)
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create a new empty validation result
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if validation passed (no errors)
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Add an error
    pub fn add_error(&mut self, error: YamlError) {
        self.errors.push(error);
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Merge another validation result into this one
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

/// Validate a YAML schematic definition
pub fn validate(schematic: &YamlSchematic) -> ValidationResult {
    let mut result = ValidationResult::new();

    // Validate paper size
    validate_paper_size(&schematic.meta.paper, &mut result);

    // Get all components (merged from groups and top-level)
    let all_components = schematic.all_components();
    let all_connections = schematic.all_connections();

    // Check for duplicate references
    validate_no_duplicate_references(schematic, &mut result);

    // Validate each component
    for (reference, component) in &all_components {
        validate_component(reference, component, &mut result);
    }

    // Validate each connection
    for (index, connection) in all_connections.iter().enumerate() {
        validate_connection(index, connection, &all_components, &mut result);
    }

    result
}

/// Validate a YAML schematic with deep symbol/pin checking
/// This looks up actual symbols from KiCAD libraries and verifies pins exist
pub fn validate_deep(schematic: &YamlSchematic, config: &KicadConfig) -> ValidationResult {
    // First do basic validation
    let mut result = validate(schematic);

    // Get all components
    let all_components = schematic.all_components();
    let all_connections = schematic.all_connections();

    // Look up symbols and cache them
    let mut symbols: HashMap<String, Symbol> = HashMap::new();
    let mut failed_lookups: HashSet<String> = HashSet::new();

    for (reference, component) in &all_components {
        let parts: Vec<&str> = component.symbol.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue; // Already caught by basic validation
        }
        let library = parts[0];
        let symbol_name = parts[1];

        // Try to look up the symbol
        match find_symbol(config, library, symbol_name) {
            Ok(symbol) => {
                symbols.insert(reference.clone(), symbol);
            }
            Err(_) => {
                result.add_error(YamlError::SymbolNotFound {
                    library: library.to_string(),
                    symbol: symbol_name.to_string(),
                });
                failed_lookups.insert(reference.clone());
            }
        }
    }

    // Validate pins in connections against actual symbols
    for connection in &all_connections {
        for pin_ref in &connection.pins {
            let parts: Vec<&str> = pin_ref.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue; // Already caught by basic validation
            }
            let reference = parts[0];
            let pin = parts[1];

            // Skip if symbol lookup failed
            if failed_lookups.contains(reference) {
                continue;
            }

            // Check if we have the symbol
            if let Some(symbol) = symbols.get(reference) {
                // Check if the pin exists on the symbol
                let pin_exists = symbol
                    .units
                    .iter()
                    .flat_map(|u| u.pins.iter())
                    .any(|p| pin_matches(pin, &p.number.number, &p.name.name));

                if !pin_exists {
                    // Collect available pins for error message
                    let available: Vec<String> = symbol
                        .units
                        .iter()
                        .flat_map(|u| u.pins.iter())
                        .map(|p| {
                            if p.name.name.is_empty() || p.name.name == "~" {
                                p.number.number.clone()
                            } else {
                                format!("{} ({})", p.number.number, p.name.name)
                            }
                        })
                        .collect();

                    result.add_error(YamlError::PinNotFound {
                        reference: reference.to_string(),
                        pin: format!("{} - available pins: {}", pin, available.join(", ")),
                    });
                }
            }
        }

        for pin_ref in &connection.no_connect {
            let parts: Vec<&str> = pin_ref.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let reference = parts[0];
            let pin = parts[1];

            if failed_lookups.contains(reference) {
                continue;
            }

            if let Some(symbol) = symbols.get(reference) {
                let pin_exists = symbol
                    .units
                    .iter()
                    .flat_map(|u| u.pins.iter())
                    .any(|p| pin_matches(pin, &p.number.number, &p.name.name));

                if !pin_exists {
                    let available: Vec<String> = symbol
                        .units
                        .iter()
                        .flat_map(|u| u.pins.iter())
                        .map(|p| {
                            if p.name.name.is_empty() || p.name.name == "~" {
                                p.number.number.clone()
                            } else {
                                format!("{} ({})", p.number.number, p.name.name)
                            }
                        })
                        .collect();

                    result.add_error(YamlError::PinNotFound {
                        reference: reference.to_string(),
                        pin: format!("{} - available pins: {}", pin, available.join(", ")),
                    });
                }
            }
        }
    }

    result
}

/// Validate paper size
fn validate_paper_size(paper: &str, result: &mut ValidationResult) {
    let valid_sizes = [
        "A4", "A3", "A2", "A1", "A0", "A", "B", "C", "D", "E", "USLetter", "USLegal", "USLedger",
    ];
    if !valid_sizes.contains(&paper) {
        result.add_warning(format!(
            "Unknown paper size '{}', using A4. Valid sizes: {}",
            paper,
            valid_sizes.join(", ")
        ));
    }
}

/// Check for duplicate component references
fn validate_no_duplicate_references(schematic: &YamlSchematic, result: &mut ValidationResult) {
    let mut seen = HashSet::new();

    // Check top-level components
    for reference in schematic.components.keys() {
        if !seen.insert(reference.clone()) {
            result.add_error(YamlError::DuplicateReference(reference.clone()));
        }
    }

    // Check group components
    for (_group_name, group) in &schematic.groups {
        for reference in group.components.keys() {
            if !seen.insert(reference.clone()) {
                result.add_error(YamlError::DuplicateReference(reference.clone()));
            }
        }
    }
}

/// Validate a component definition
fn validate_component(
    reference: &str,
    component: &super::types::ComponentDef,
    result: &mut ValidationResult,
) {
    // Validate symbol format (Library:Symbol)
    if !component.symbol.contains(':') {
        result.add_error(YamlError::InvalidSymbolFormat {
            symbol: component.symbol.clone(),
            component: reference.to_string(),
        });
    } else {
        let parts: Vec<&str> = component.symbol.splitn(2, ':').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            result.add_error(YamlError::InvalidSymbolFormat {
                symbol: component.symbol.clone(),
                component: reference.to_string(),
            });
        }
    }

    // Validate angle (must be 0, 90, 180, or 270)
    let valid_angles = [0.0, 90.0, 180.0, 270.0];
    if !valid_angles.contains(&component.angle) {
        result.add_error(YamlError::InvalidAngle {
            component: reference.to_string(),
            angle: component.angle,
        });
    }

    // Validate mirror setting
    if let Some(ref mirror) = component.mirror {
        let mirror_lower = mirror.to_lowercase();
        if mirror_lower != "x" && mirror_lower != "y" {
            result.add_error(YamlError::InvalidMirror {
                component: reference.to_string(),
                value: mirror.clone(),
            });
        }
    }

    // Validate unit number
    if component.unit == 0 {
        result.add_warning(format!(
            "Component '{}' has unit 0, should be >= 1. Using 1.",
            reference
        ));
    }
}

/// Validate a connection definition
fn validate_connection(
    index: usize,
    connection: &super::types::Connection,
    all_components: &std::collections::HashMap<String, super::types::ComponentDef>,
    result: &mut ValidationResult,
) {
    let has_wire_endpoints = connection.pins.len() >= 2 || connection.net.is_some();
    let has_no_connect = !connection.no_connect.is_empty();

    if !has_wire_endpoints && !has_no_connect {
        result.add_error(YamlError::InsufficientEndpoints {
            connection_index: index,
        });
    }

    if connection.pins.is_empty() && connection.net.is_none() && !has_no_connect {
        result.add_error(YamlError::InsufficientEndpoints {
            connection_index: index,
        });
        return;
    }

    // Validate each pin reference format and check component exists
    for pin_ref in &connection.pins {
        validate_pin_reference(pin_ref, all_components, result);
    }
    for pin_ref in &connection.no_connect {
        validate_pin_reference(pin_ref, all_components, result);
    }
}

/// Validate a pin reference (REF:pin format)
fn validate_pin_reference(
    pin_ref: &str,
    all_components: &std::collections::HashMap<String, super::types::ComponentDef>,
    result: &mut ValidationResult,
) {
    let parts: Vec<&str> = pin_ref.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        result.add_error(YamlError::InvalidPinReference {
            pin_ref: pin_ref.to_string(),
            component: String::new(),
        });
        return;
    }

    let reference = parts[0];
    // Check that the component exists in our definitions
    if !all_components.contains_key(reference) {
        result.add_warning(format!(
            "Pin reference '{}' refers to component '{}' not defined in YAML. \
             Component may already exist in schematic or this is an error.",
            pin_ref, reference
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::types::YamlSchematic;

    #[test]
    fn test_valid_schematic() {
        let yaml = r#"
meta:
  paper: A4

components:
  R1:
    symbol: Device:R
    position: [100, 50]
    angle: 0

connections:
  - pins: [R1:1, R1:2]
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        assert!(result.is_valid(), "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_invalid_symbol_format() {
        let yaml = r#"
components:
  R1:
    symbol: InvalidFormat
    position: [100, 50]
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        assert!(!result.is_valid());
        assert!(matches!(
            result.errors[0],
            YamlError::InvalidSymbolFormat { .. }
        ));
    }

    #[test]
    fn test_invalid_angle() {
        let yaml = r#"
components:
  R1:
    symbol: Device:R
    position: [100, 50]
    angle: 45
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        assert!(!result.is_valid());
        assert!(matches!(result.errors[0], YamlError::InvalidAngle { .. }));
    }

    #[test]
    fn test_duplicate_reference() {
        let yaml = r#"
groups:
  Group1:
    components:
      R1:
        symbol: Device:R
        position: [100, 50]

components:
  R1:
    symbol: Device:R
    position: [150, 50]
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        assert!(!result.is_valid());
        assert!(matches!(
            result.errors[0],
            YamlError::DuplicateReference(_)
        ));
    }

    #[test]
    fn test_insufficient_endpoints() {
        let yaml = r#"
components:
  R1:
    symbol: Device:R
    position: [100, 50]

connections:
  - pins: [R1:1]
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        assert!(!result.is_valid());
        assert!(matches!(
            result.errors[0],
            YamlError::InsufficientEndpoints { .. }
        ));
    }

    #[test]
    fn test_single_pin_with_net_valid() {
        let yaml = r#"
components:
  R1:
    symbol: Device:R
    position: [100, 50]

connections:
  - net: GND
    pins: [R1:2]
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        assert!(result.is_valid(), "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_invalid_mirror() {
        let yaml = r#"
components:
  R1:
    symbol: Device:R
    position: [100, 50]
    mirror: z
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        assert!(!result.is_valid());
        assert!(matches!(result.errors[0], YamlError::InvalidMirror { .. }));
    }

    #[test]
    fn test_warning_for_unknown_component() {
        let yaml = r#"
components:
  R1:
    symbol: Device:R
    position: [100, 50]

connections:
  - pins: [R1:1, UNKNOWN:2]
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let result = validate(&schematic);
        // This should produce a warning, not an error
        assert!(result.is_valid());
        assert!(!result.warnings.is_empty());
    }
}
