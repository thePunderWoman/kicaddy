//! YAML to KiCAD schematic compiler

use std::collections::HashMap;

use crate::commands::{snap_to_grid, Command, ConnectCommand};
use crate::common::{Point, Position};
use crate::config::KicadConfig;
use crate::layout::{paper_dimensions, ForceDirectedLayout, LayoutConfig, LayoutEdge, LayoutGraph, LayoutNode, PinInfo};
use crate::schematic::{Mirror, PaperSize, Schematic, TitleBlock};
use crate::symbol::lookup::find_symbol;
use crate::symbol::Symbol;

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

/// Information about a component's layout position (for debugging)
#[derive(Debug, Clone)]
pub struct LayoutInfo {
    /// Component reference (e.g., "R1", "U1")
    pub reference: String,
    /// Position in mm
    pub position_mm: Point,
    /// Position in mils (for KiCAD comparison)
    pub position_mils: (i32, i32),
    /// Bounding box size in mm
    pub size_mm: (f64, f64),
    /// Bounding box size in mils
    pub size_mils: (i32, i32),
    /// Group this component belongs to (if any)
    pub group: Option<String>,
    /// Whether position was fixed (user-specified)
    pub fixed: bool,
}

/// Output from computing layout (for debugging)
#[derive(Debug, Clone)]
pub struct LayoutOutput {
    /// Information about each component
    pub components: Vec<LayoutInfo>,
    /// Total bounding box of all components (min_x, min_y, max_x, max_y) in mils
    pub bounding_box_mils: (i32, i32, i32, i32),
    /// Paper dimensions in mils
    pub paper_mils: (i32, i32),
    /// Paper name
    pub paper_name: String,
    /// Overlapping component pairs (if any)
    pub overlaps: Vec<(String, String)>,
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

        // Check if any components need auto-layout
        let needs_layout = all_components.values().any(|c| c.position.is_none());

        // Look up all symbols (needed for both placement and layout)
        let mut symbols: HashMap<String, Symbol> = HashMap::new();
        for (reference, component) in &all_components {
            let parts: Vec<&str> = component.symbol.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let library = parts[0];
            let symbol_name = parts[1];

            if let Ok(symbol) = find_symbol(&self.config, library, symbol_name) {
                symbols.insert(reference.clone(), symbol);
            }
        }

        // Compute layout positions if needed
        let computed_positions = if needs_layout {
            self.compute_layout(yaml_sch, &all_components, &all_connections, &symbols)?
        } else {
            HashMap::new()
        };

        // Place all components
        for (reference, component) in &all_components {
            let position = computed_positions.get(reference).cloned();
            let lib_id = self.place_component(&mut schematic, reference, component, position)?;
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

    /// Get layout information without compiling (for debugging layout algorithm)
    pub fn get_layout_info(&self, yaml_sch: &YamlSchematic) -> Result<LayoutOutput, YamlError> {
        // Get all components and connections
        let all_components = yaml_sch.all_components();
        let all_connections = yaml_sch.all_connections();

        // Build a map of component reference -> group name
        let mut component_groups: HashMap<String, String> = HashMap::new();
        for (group_name, group) in &yaml_sch.groups {
            for reference in group.components.keys() {
                component_groups.insert(reference.clone(), group_name.clone());
            }
        }

        // Look up all symbols
        let mut symbols: HashMap<String, Symbol> = HashMap::new();
        for (reference, component) in &all_components {
            let parts: Vec<&str> = component.symbol.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let library = parts[0];
            let symbol_name = parts[1];

            if let Ok(symbol) = find_symbol(&self.config, library, symbol_name) {
                symbols.insert(reference.clone(), symbol);
            }
        }

        // Build the layout graph
        let mut graph = LayoutGraph::new();

        for (reference, component) in &all_components {
            let symbol = symbols.get(reference);
            let (width, height) = Self::calculate_symbol_bounds(symbol);

            let mut node = LayoutNode::new(reference.clone(), (width, height));

            // Set group if component is in a group
            if let Some(group_name) = component_groups.get(reference) {
                node = node.with_group(Some(group_name.clone()));
            }

            // If position is specified, mark as fixed
            if let Some(ref pos) = component.position {
                node = node.with_position(pos.x(), pos.y()).with_fixed(true);
            }

            // Add pin information
            if let Some(sym) = symbol {
                for unit in &sym.units {
                    for pin in &unit.pins {
                        let pin_info = PinInfo {
                            offset: Point::new(pin.position.x, pin.position.y),
                            direction: pin.position.angle,
                            number: pin.number.number.clone(),
                            name: pin.name.name.clone(),
                        };
                        node.add_pin(pin.number.number.clone(), pin_info.clone());
                        if !pin.name.name.is_empty() && pin.name.name != "~" {
                            node.add_pin(pin.name.name.clone(), pin_info);
                        }
                    }
                }
            }

            graph.add_node(node);
        }

        // Create edges from connections
        for connection in &all_connections {
            for i in 0..connection.pins.len() {
                for j in (i + 1)..connection.pins.len() {
                    let pin1 = &connection.pins[i];
                    let pin2 = &connection.pins[j];

                    let parts1: Vec<&str> = pin1.splitn(2, ':').collect();
                    let parts2: Vec<&str> = pin2.splitn(2, ':').collect();

                    if parts1.len() == 2 && parts2.len() == 2 {
                        let edge = LayoutEdge::new(
                            parts1[0].to_string(),
                            parts1[1].to_string(),
                            parts2[0].to_string(),
                            parts2[1].to_string(),
                        );
                        graph.add_edge(edge);
                    }
                }
            }
        }

        // Get layout configuration and paper dimensions
        let config = yaml_sch.meta.layout.clone().unwrap_or_else(LayoutConfig::default);
        let (paper_width, paper_height) = paper_dimensions(&yaml_sch.meta.paper);

        // Run the force-directed layout
        let layout = ForceDirectedLayout::with_config(config);
        layout.layout_for_paper(&mut graph, paper_width, paper_height);

        // Convert mm to mils (1 mm = 39.3701 mils)
        const MM_TO_MILS: f64 = 39.3701;

        // Build output
        let mut components: Vec<LayoutInfo> = Vec::new();
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        for (reference, node) in &graph.nodes {
            let pos_mils_x = (node.position.x * MM_TO_MILS).round() as i32;
            let pos_mils_y = (node.position.y * MM_TO_MILS).round() as i32;
            let size_mils_w = (node.size.0 * MM_TO_MILS).round() as i32;
            let size_mils_h = (node.size.1 * MM_TO_MILS).round() as i32;

            // Update bounding box (using component bounds, not just center)
            let half_w = node.size.0 / 2.0;
            let half_h = node.size.1 / 2.0;
            min_x = min_x.min(node.position.x - half_w);
            min_y = min_y.min(node.position.y - half_h);
            max_x = max_x.max(node.position.x + half_w);
            max_y = max_y.max(node.position.y + half_h);

            components.push(LayoutInfo {
                reference: reference.clone(),
                position_mm: node.position,
                position_mils: (pos_mils_x, pos_mils_y),
                size_mm: node.size,
                size_mils: (size_mils_w, size_mils_h),
                group: node.group.clone(),
                fixed: node.fixed,
            });
        }

        // Sort components by reference for consistent output
        components.sort_by(|a, b| a.reference.cmp(&b.reference));

        // Check for overlaps
        let mut overlaps: Vec<(String, String)> = Vec::new();
        let refs: Vec<&String> = graph.nodes.keys().collect();
        for i in 0..refs.len() {
            for j in (i + 1)..refs.len() {
                let node_a = graph.get_node(refs[i]).unwrap();
                let node_b = graph.get_node(refs[j]).unwrap();
                if node_a.overlaps(node_b, 0.0) {
                    overlaps.push((refs[i].clone(), refs[j].clone()));
                }
            }
        }

        let bounding_box_mils = (
            (min_x * MM_TO_MILS).round() as i32,
            (min_y * MM_TO_MILS).round() as i32,
            (max_x * MM_TO_MILS).round() as i32,
            (max_y * MM_TO_MILS).round() as i32,
        );

        let paper_mils = (
            (paper_width * MM_TO_MILS).round() as i32,
            (paper_height * MM_TO_MILS).round() as i32,
        );

        Ok(LayoutOutput {
            components,
            bounding_box_mils,
            paper_mils,
            paper_name: yaml_sch.meta.paper.clone(),
            overlaps,
        })
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
        computed_position: Option<Point>,
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

        // Determine position: use YAML position if specified, otherwise use computed position
        let (x, y) = if let Some(ref pos) = component.position {
            (snap_to_grid(pos.x()), snap_to_grid(pos.y()))
        } else if let Some(computed) = computed_position {
            (snap_to_grid(computed.x), snap_to_grid(computed.y))
        } else {
            // Fallback: place at origin (should not happen if layout ran)
            (0.0, 0.0)
        };
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

    /// Compute layout positions for components that don't have explicit positions
    fn compute_layout(
        &self,
        yaml_sch: &YamlSchematic,
        all_components: &HashMap<String, ComponentDef>,
        all_connections: &[super::types::Connection],
        symbols: &HashMap<String, Symbol>,
    ) -> Result<HashMap<String, Point>, YamlError> {
        let mut graph = LayoutGraph::new();

        // Build a map of component reference -> group name
        let mut component_groups: HashMap<String, String> = HashMap::new();
        for (group_name, group) in &yaml_sch.groups {
            for reference in group.components.keys() {
                component_groups.insert(reference.clone(), group_name.clone());
            }
        }

        // Create nodes for each component
        for (reference, component) in all_components {
            let symbol = symbols.get(reference);
            let (width, height) = Self::calculate_symbol_bounds(symbol);

            let mut node = LayoutNode::new(reference.clone(), (width, height));

            // Set group if component is in a group
            if let Some(group_name) = component_groups.get(reference) {
                node = node.with_group(Some(group_name.clone()));
            }

            // If position is specified, mark as fixed
            if let Some(ref pos) = component.position {
                node = node.with_position(pos.x(), pos.y()).with_fixed(true);
            }

            // Add pin information
            if let Some(sym) = symbol {
                for unit in &sym.units {
                    for pin in &unit.pins {
                        let pin_info = PinInfo {
                            offset: Point::new(pin.position.x, pin.position.y),
                            direction: pin.position.angle,
                            number: pin.number.number.clone(),
                            name: pin.name.name.clone(),
                        };

                        // Add both by number and by name for flexible lookup
                        node.add_pin(pin.number.number.clone(), pin_info.clone());
                        if !pin.name.name.is_empty() && pin.name.name != "~" {
                            node.add_pin(pin.name.name.clone(), pin_info);
                        }
                    }
                }
            }

            graph.add_node(node);
        }

        // Create edges from connections
        for connection in all_connections {
            // Create edges between all pairs of pins in the connection
            for i in 0..connection.pins.len() {
                for j in (i + 1)..connection.pins.len() {
                    let pin1 = &connection.pins[i];
                    let pin2 = &connection.pins[j];

                    let parts1: Vec<&str> = pin1.splitn(2, ':').collect();
                    let parts2: Vec<&str> = pin2.splitn(2, ':').collect();

                    if parts1.len() == 2 && parts2.len() == 2 {
                        let edge = LayoutEdge::new(
                            parts1[0].to_string(),
                            parts1[1].to_string(),
                            parts2[0].to_string(),
                            parts2[1].to_string(),
                        );
                        graph.add_edge(edge);
                    }
                }
            }
        }

        // Get layout configuration
        let config = yaml_sch
            .meta
            .layout
            .clone()
            .unwrap_or_else(LayoutConfig::default);

        // Get paper dimensions for centering
        let (paper_width, paper_height) = paper_dimensions(&yaml_sch.meta.paper);

        // Run the force-directed layout
        let layout = ForceDirectedLayout::with_config(config);
        layout.layout_for_paper(&mut graph, paper_width, paper_height);

        // Extract computed positions
        let mut positions = HashMap::new();
        for (reference, node) in &graph.nodes {
            positions.insert(reference.clone(), node.position);
        }

        Ok(positions)
    }

    /// Calculate bounding box size for a symbol
    fn calculate_symbol_bounds(symbol: Option<&Symbol>) -> (f64, f64) {
        let Some(sym) = symbol else {
            // Default size for unknown symbols
            return (10.0, 10.0);
        };

        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        for unit in &sym.units {
            // Process graphics
            for graphic in &unit.graphics {
                use crate::symbol::GraphicItem;
                match graphic {
                    GraphicItem::Rectangle(rect) => {
                        min_x = min_x.min(rect.start.x).min(rect.end.x);
                        max_x = max_x.max(rect.start.x).max(rect.end.x);
                        min_y = min_y.min(rect.start.y).min(rect.end.y);
                        max_y = max_y.max(rect.start.y).max(rect.end.y);
                    }
                    GraphicItem::Polyline(poly) => {
                        for pt in &poly.points {
                            min_x = min_x.min(pt.x);
                            max_x = max_x.max(pt.x);
                            min_y = min_y.min(pt.y);
                            max_y = max_y.max(pt.y);
                        }
                    }
                    GraphicItem::Circle(circle) => {
                        min_x = min_x.min(circle.center.x - circle.radius);
                        max_x = max_x.max(circle.center.x + circle.radius);
                        min_y = min_y.min(circle.center.y - circle.radius);
                        max_y = max_y.max(circle.center.y + circle.radius);
                    }
                    GraphicItem::Arc(arc) => {
                        min_x = min_x.min(arc.start.x).min(arc.mid.x).min(arc.end.x);
                        max_x = max_x.max(arc.start.x).max(arc.mid.x).max(arc.end.x);
                        min_y = min_y.min(arc.start.y).min(arc.mid.y).min(arc.end.y);
                        max_y = max_y.max(arc.start.y).max(arc.mid.y).max(arc.end.y);
                    }
                    GraphicItem::Text(_) => {}
                }
            }

            // Process pins
            for pin in &unit.pins {
                let pin_angle_rad = pin.position.angle.to_radians();
                let tip_x = pin.position.x - pin.length * pin_angle_rad.cos();
                let tip_y = pin.position.y + pin.length * pin_angle_rad.sin();

                min_x = min_x.min(pin.position.x).min(tip_x);
                max_x = max_x.max(pin.position.x).max(tip_x);
                min_y = min_y.min(pin.position.y).min(tip_y);
                max_y = max_y.max(pin.position.y).max(tip_y);
            }
        }

        // Check if we found valid bounds
        if min_x == f64::MAX || max_x == f64::MIN {
            return (10.0, 10.0);
        }

        let width = (max_x - min_x).abs();
        let height = (max_y - min_y).abs();

        // Ensure minimum size
        (width.max(5.0), height.max(5.0))
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

    #[test]
    #[ignore]
    fn test_compile_auto_layout() {
        // Test compilation with no positions (auto-layout)
        let yaml = r#"
meta:
  paper: A4
  title: "Auto-Layout Test"

components:
  R1:
    symbol: Device:R
    value: 10k

  R2:
    symbol: Device:R
    value: 4.7k

  C1:
    symbol: Device:C
    value: 100nF

connections:
  - pins: [R1:2, R2:1]
  - pins: [R2:2, C1:1]
"#;
        let result = compile_yaml_str(yaml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let output = result.unwrap();
        assert_eq!(output.components_placed.len(), 3);
        assert_eq!(output.connections_made, 2);

        // Verify symbols were placed at non-zero positions
        for sym in &output.schematic.symbols {
            // Positions should be on the grid and reasonable
            assert!(
                sym.position.x >= 0.0,
                "Symbol at negative X: {:?}",
                sym.position
            );
            assert!(
                sym.position.y >= 0.0,
                "Symbol at negative Y: {:?}",
                sym.position
            );
        }
    }

    #[test]
    #[ignore]
    fn test_compile_mixed_positions() {
        // Test compilation with some positions specified (fixed) and some auto
        let yaml = r#"
meta:
  paper: A4

components:
  R1:
    symbol: Device:R
    position: [100, 50]
    value: 10k

  R2:
    symbol: Device:R
    value: 4.7k

connections:
  - pins: [R1:2, R2:1]
"#;
        let result = compile_yaml_str(yaml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let output = result.unwrap();
        assert_eq!(output.components_placed.len(), 2);

        // Find R1 and verify its position is preserved
        let r1 = output.schematic.symbols.iter().find(|s| {
            s.properties
                .iter()
                .any(|p| p.name == "Reference" && p.value == "R1")
        });
        assert!(r1.is_some(), "R1 not found");

        // R1 should be near the specified position (snapped to grid)
        let r1 = r1.unwrap();
        assert!(
            (r1.position.x - 100.0).abs() < 2.0,
            "R1 X position changed: {}",
            r1.position.x
        );
        assert!(
            (r1.position.y - 50.0).abs() < 2.0,
            "R1 Y position changed: {}",
            r1.position.y
        );
    }

    #[test]
    #[ignore]
    fn test_compile_grouped_components() {
        // Test that grouped components cluster together
        let yaml = r#"
meta:
  paper: A4

groups:
  Power:
    components:
      C1:
        symbol: Device:C
        value: 10uF
      C2:
        symbol: Device:C
        value: 10uF

  Signal:
    components:
      R1:
        symbol: Device:R
        value: 10k
      R2:
        symbol: Device:R
        value: 10k

connections:
  - pins: [C1:1, C2:1]
  - pins: [R1:2, R2:1]
"#;
        let result = compile_yaml_str(yaml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let output = result.unwrap();
        assert_eq!(output.components_placed.len(), 4);
    }
}
