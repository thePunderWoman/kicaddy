//! YAML to KiCAD schematic compiler

use std::collections::HashMap;

use crate::commands::{snap_to_grid, Command, ConnectCommand};
use crate::common::Position;
use crate::config::KicadConfig;
use crate::schematic::{Mirror, PaperSize, Schematic, TitleBlock};
use crate::symbol::lookup::find_symbol;

use super::error::YamlError;
use super::types::{ComponentDef, YamlSchematic};
use super::validation::{validate_deep, ValidationResult};

/// Output from compiling a YAML schematic
#[derive(Debug, Clone)]
pub struct CompileOutput {
    /// The compiled schematic
    pub schematic: Schematic,
    /// Components that were placed (reference -> lib_id)
    pub components_placed: HashMap<String, String>,
    /// Connections that were made
    pub connections_made: usize,
    /// Validation warnings (if any)
    pub warnings: Vec<String>,
}

/// Compiler for YAML schematic definitions
pub struct Compiler {
    config: KicadConfig,
}

impl Compiler {
    /// Create a new compiler with auto-detected KiCAD configuration
    pub fn new() -> Result<Self, YamlError> {
        let config = KicadConfig::detect()?;
        Ok(Self { config })
    }

    /// Create a new compiler with a specific KiCAD configuration
    pub fn with_config(config: KicadConfig) -> Self {
        Self { config }
    }

    /// Compile a YAML schematic definition into a KiCAD schematic
    pub fn compile(&self, yaml_sch: &YamlSchematic) -> Result<CompileOutput, YamlError> {
        // First validate the YAML schematic with deep symbol/pin checking
        let validation = validate_deep(yaml_sch, &self.config);
        if !validation.is_valid() {
            // Return the first error
            return Err(validation.errors.into_iter().next().unwrap());
        }

        let mut schematic = Schematic::new();
        let mut components_placed = HashMap::new();
        let mut connections_made = 0;

        // Apply meta settings
        self.apply_meta(&mut schematic, &yaml_sch.meta)?;

        // Get all components and connections (merged from groups and top-level)
        let all_components = yaml_sch.all_components();
        let all_connections = yaml_sch.all_connections();

        // Place all components
        for (reference, component) in &all_components {
            let lib_id = self.place_component(&mut schematic, reference, component)?;
            components_placed.insert(reference.clone(), lib_id);
        }

        // Create all connections
        for connection in &all_connections {
            self.create_connection(&mut schematic, connection)?;
            connections_made += 1;
        }

        Ok(CompileOutput {
            schematic,
            components_placed,
            connections_made,
            warnings: validation.warnings,
        })
    }

    /// Validate a YAML schematic without compiling (includes deep symbol/pin checking)
    pub fn validate(&self, yaml_sch: &YamlSchematic) -> ValidationResult {
        validate_deep(yaml_sch, &self.config)
    }

    /// Apply meta settings to the schematic
    fn apply_meta(
        &self,
        schematic: &mut Schematic,
        meta: &super::types::Meta,
    ) -> Result<(), YamlError> {
        // Set paper size
        schematic.paper = PaperSize::from_str(&meta.paper);

        // Set title block if any fields are present
        if meta.title.is_some()
            || meta.author.is_some()
            || meta.revision.is_some()
            || meta.date.is_some()
            || meta.company.is_some()
        {
            let mut comments = Vec::new();
            if let Some(ref author) = meta.author {
                comments.push((1, author.clone()));
            }

            schematic.title_block = Some(TitleBlock {
                title: meta.title.clone(),
                date: meta.date.clone(),
                rev: meta.revision.clone(),
                company: meta.company.clone(),
                comments,
            });
        }

        Ok(())
    }

    /// Place a single component into the schematic
    fn place_component(
        &self,
        schematic: &mut Schematic,
        reference: &str,
        component: &ComponentDef,
    ) -> Result<String, YamlError> {
        // Parse library:symbol
        let parts: Vec<&str> = component.symbol.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(YamlError::InvalidSymbolFormat {
                symbol: component.symbol.clone(),
                component: reference.to_string(),
            });
        }
        let library = parts[0];
        let symbol_name = parts[1];

        // Look up the symbol from the library
        let symbol = find_symbol(&self.config, library, symbol_name).map_err(|_| {
            YamlError::SymbolNotFound {
                library: library.to_string(),
                symbol: symbol_name.to_string(),
            }
        })?;

        // Snap position to grid
        let x = snap_to_grid(component.position.x());
        let y = snap_to_grid(component.position.y());
        let position = Position::new(x, y, component.angle);

        // Add the symbol to the schematic
        let lib_id = format!("{}:{}", library, symbol_name);
        let actual_ref =
            schematic.add_symbol(&symbol, &lib_id, position, Some(reference), component.value.as_deref());

        // Apply mirror setting if specified
        if let Some(ref mirror_str) = component.mirror {
            let mirror = match mirror_str.to_lowercase().as_str() {
                "x" => Some(Mirror::X),
                "y" => Some(Mirror::Y),
                _ => None,
            };

            if let Some(m) = mirror {
                // Find the symbol we just added and set its mirror
                if let Some(sym) = schematic
                    .symbols
                    .iter_mut()
                    .find(|s| {
                        s.properties
                            .iter()
                            .any(|p| p.name == "Reference" && p.value == actual_ref)
                    })
                {
                    sym.mirror = Some(m);
                }
            }
        }

        Ok(lib_id)
    }

    /// Create a connection between pins and/or net
    fn create_connection(
        &self,
        schematic: &mut Schematic,
        connection: &super::types::Connection,
    ) -> Result<(), YamlError> {
        // Build endpoint list for ConnectCommand
        let mut endpoints: Vec<String> = Vec::new();

        // Add pin references
        for pin_ref in &connection.pins {
            endpoints.push(pin_ref.clone());
        }

        // Add net as endpoint if present
        if let Some(ref net) = connection.net {
            // Use '&' prefix for net names in ConnectCommand format
            endpoints.push(format!("&{}", net));
        }

        // ConnectCommand handles the logic of wiring pins and creating labels
        if endpoints.len() >= 2 || (endpoints.len() == 1 && connection.net.is_some()) {
            // If only one pin with a net, we need both for connect
            if endpoints.len() < 2 {
                // Single pin to net: endpoint list is [pin, &net] which we already built
            }

            let cmd = ConnectCommand { endpoints };
            cmd.execute(schematic)
                .map_err(|e| YamlError::Other(e.to_string()))?;
        }

        Ok(())
    }
}

/// Convenience function to compile a YAML schematic from a string
pub fn compile_yaml_str(yaml: &str) -> Result<CompileOutput, YamlError> {
    let yaml_sch = YamlSchematic::from_str(yaml)?;
    let compiler = Compiler::new()?;
    compiler.compile(&yaml_sch)
}

/// Convenience function to compile a YAML schematic from a file
pub fn compile_yaml_file(path: &std::path::Path) -> Result<CompileOutput, YamlError> {
    let yaml_sch = YamlSchematic::from_file(path)?;
    let compiler = Compiler::new()?;
    compiler.compile(&yaml_sch)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require KiCAD to be installed
    // They're ignored by default to avoid CI failures

    #[test]
    #[ignore]
    fn test_compile_basic_schematic() {
        let yaml = r#"
meta:
  paper: A4
  title: "Test Circuit"

components:
  R1:
    symbol: Device:R
    position: [100, 50]
    value: 10k

  R2:
    symbol: Device:R
    position: [100, 70]
    value: 4.7k

connections:
  - pins: [R1:2, R2:1]
"#;
        let result = compile_yaml_str(yaml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let output = result.unwrap();
        assert_eq!(output.components_placed.len(), 2);
        assert_eq!(output.connections_made, 1);
        assert!(output.schematic.symbols.len() >= 2);
    }

    #[test]
    #[ignore]
    fn test_compile_with_net_labels() {
        let yaml = r#"
components:
  R1:
    symbol: Device:R
    position: [100, 50]

connections:
  - net: GND
    pins: [R1:2]
"#;
        let result = compile_yaml_str(yaml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let output = result.unwrap();
        // GND is a power net, should create a global label
        assert!(!output.schematic.global_labels.is_empty() || !output.schematic.labels.is_empty());
    }

    #[test]
    fn test_compile_invalid_symbol() {
        let yaml = r#"
components:
  R1:
    symbol: InvalidLibrary:InvalidSymbol
    position: [100, 50]
"#;
        let yaml_sch = YamlSchematic::from_str(yaml).unwrap();
        let compiler = Compiler::new();
        if compiler.is_err() {
            // KiCAD not installed, skip
            return;
        }

        let result = compiler.unwrap().compile(&yaml_sch);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_only() {
        use crate::yaml::validate;

        let yaml = r#"
components:
  R1:
    symbol: InvalidFormat
    position: [100, 50]
"#;
        let yaml_sch = YamlSchematic::from_str(yaml).unwrap();
        let validation = validate(&yaml_sch);
        assert!(!validation.is_valid());
    }
}
