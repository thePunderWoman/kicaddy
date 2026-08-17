//! YAML schema types for declarative schematic definition

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::layout::LayoutConfig;

/// Root structure for a YAML schematic definition
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YamlSchematic {
    /// Schematic metadata (paper size, title, etc.)
    #[serde(default)]
    pub meta: Meta,

    /// Logical groups of components and connections (optional)
    #[serde(default)]
    pub groups: HashMap<String, Group>,

    /// Top-level components (merged with group components)
    #[serde(default)]
    pub components: HashMap<String, ComponentDef>,

    /// Hierarchical sheet definitions keyed by sheet name
    #[serde(default)]
    pub sheets: HashMap<String, SheetDef>,

    /// Top-level connections (merged with group connections)
    #[serde(default)]
    pub connections: Vec<Connection>,
}

/// Schematic metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Meta {
    /// Paper size (A4, A3, etc.)
    #[serde(default = "default_paper")]
    pub paper: String,

    /// Schematic title
    #[serde(default)]
    pub title: Option<String>,

    /// Author name
    #[serde(default)]
    pub author: Option<String>,

    /// Revision
    #[serde(default)]
    pub revision: Option<String>,

    /// Date
    #[serde(default)]
    pub date: Option<String>,

    /// Company
    #[serde(default)]
    pub company: Option<String>,

    /// Layout configuration for automatic component positioning
    #[serde(default)]
    pub layout: Option<LayoutConfig>,
}

fn default_paper() -> String {
    "A4".to_string()
}

/// A logical group of related components and connections
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Group {
    /// Components in this group
    #[serde(default)]
    pub components: HashMap<String, ComponentDef>,

    /// Connections within this group
    #[serde(default)]
    pub connections: Vec<Connection>,
}

/// Component definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDef {
    /// Symbol in Library:Symbol format (e.g., "Device:R", "Regulator_Linear:AP2112K-3.3")
    pub symbol: String,

    /// Position as [x, y] or {x: _, y: _}. Optional - if omitted, auto-layout will be used.
    #[serde(default)]
    pub position: Option<Position2D>,

    /// Rotation angle in degrees (0, 90, 180, 270)
    #[serde(default)]
    pub angle: f64,

    /// Mirror setting ("x" or "y")
    #[serde(default)]
    pub mirror: Option<String>,

    /// Component value (e.g., "10k", "100nF")
    #[serde(default)]
    pub value: Option<String>,

    /// Unit number for multi-unit symbols (default: 1)
    #[serde(default = "default_unit")]
    pub unit: u32,

    /// Hierarchical sheet this component belongs to
    #[serde(default)]
    pub sheet: Option<String>,

    /// Additional properties (Footprint, etc.)
    #[serde(default)]
    pub properties: HashMap<String, String>,
}

/// Hierarchical sheet definition for a sub-schematic
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SheetDef {
    /// Path or filename of the sub-schematic, e.g. "power" or "subcircuits/adc"
    #[serde(default)]
    pub path: Option<String>,

    /// Position in schematic coordinates
    #[serde(default)]
    pub position: Option<Position2D>,

    /// Width and height in schematic units
    #[serde(default)]
    pub size: Option<[f64; 2]>,

    /// Connection pins for this sheet symbol
    #[serde(default)]
    pub pins: Vec<SheetPinDef>,
}

/// Definition for a sheet symbol pin
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SheetPinDef {
    /// Pin name shown on the sheet symbol
    #[serde(default)]
    pub name: String,

    /// Pin shape, such as input/output/bidirectional
    #[serde(default)]
    pub shape: Option<String>,

    /// Position in schematic coordinates
    #[serde(default)]
    pub position: Option<Position2D>,
}

fn default_unit() -> u32 {
    1
}

/// Position that can be specified as [x, y] array or {x, y} object
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Position2D {
    /// Array format: [x, y]
    Array([f64; 2]),
    /// Object format: {x: _, y: _}
    Object { x: f64, y: f64 },
}

impl Position2D {
    /// Get X coordinate
    pub fn x(&self) -> f64 {
        match self {
            Position2D::Array(arr) => arr[0],
            Position2D::Object { x, .. } => *x,
        }
    }

    /// Get Y coordinate
    pub fn y(&self) -> f64 {
        match self {
            Position2D::Array(arr) => arr[1],
            Position2D::Object { y, .. } => *y,
        }
    }
}

impl Default for Position2D {
    fn default() -> Self {
        Position2D::Array([0.0, 0.0])
    }
}

/// Connection definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// Named net (optional). If provided, creates labels.
    #[serde(default)]
    pub net: Option<String>,

    /// Pin references in "REF:pin" format (e.g., ["R1:1", "C1:2", "U1:10"])
    pub pins: Vec<String>,

    /// Force global label (otherwise auto-detected from net name)
    #[serde(default)]
    pub global: Option<bool>,
}

impl YamlSchematic {
    /// Create a new empty YAML schematic
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a YAML schematic from a string
    pub fn from_str(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Parse a YAML schematic from a file
    pub fn from_file(path: &std::path::Path) -> Result<Self, crate::yaml::YamlError> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    /// Serialize to YAML string
    pub fn to_yaml_string(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Get all components (merged from groups and top-level)
    pub fn all_components(&self) -> HashMap<String, ComponentDef> {
        let mut all = self.components.clone();
        for (_group_name, group) in &self.groups {
            for (ref_name, comp) in &group.components {
                all.insert(ref_name.clone(), comp.clone());
            }
        }
        all
    }

    /// Get all connections (merged from groups and top-level)
    pub fn all_connections(&self) -> Vec<Connection> {
        let mut all = self.connections.clone();
        for (_group_name, group) in &self.groups {
            all.extend(group.connections.clone());
        }
        all
    }
}

/// Template types for init-yaml command
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YamlTemplate {
    /// Basic empty schematic
    Basic,
    /// Simple voltage regulator circuit
    Regulator,
    /// LED with resistor
    Led,
}

impl YamlTemplate {
    /// Get template content as YAML string
    pub fn content(&self) -> &'static str {
        match self {
            YamlTemplate::Basic => TEMPLATE_BASIC,
            YamlTemplate::Regulator => TEMPLATE_REGULATOR,
            YamlTemplate::Led => TEMPLATE_LED,
        }
    }

    /// Parse template name from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "basic" | "empty" => Some(YamlTemplate::Basic),
            "regulator" | "ldo" => Some(YamlTemplate::Regulator),
            "led" | "blinky" => Some(YamlTemplate::Led),
            _ => None,
        }
    }
}

const TEMPLATE_BASIC: &str = r#"# KiCAD Schematic Definition
# Generated by kicaddy

meta:
  paper: A4
  title: "My Circuit"
  author: "kicaddy"

# Define components
components:
  # Example: R1:
  #   symbol: Device:R
  #   position: [100, 50]
  #   value: 10k

# Define connections
connections:
  # Example: - pins: [R1:1, U1:2]
  # Example: - net: GND
  #            pins: [R1:2, C1:2]
"#;

const TEMPLATE_REGULATOR: &str = r#"# 3.3V LDO Regulator Circuit
# Generated by kicaddy

meta:
  paper: A4
  title: "3.3V Power Supply"
  author: "kicaddy"

groups:
  Power Supply:
    components:
      U1:
        symbol: Regulator_Linear:AP2112K-3.3
        position: [100, 50]
        value: AP2112K-3.3

      C_IN:
        symbol: Device:C
        position: [80, 60]
        value: 10uF

      C_OUT:
        symbol: Device:C
        position: [120, 60]
        value: 10uF

    connections:
      - net: VIN
        pins: [C_IN:1, U1:VIN]
      - net: GND
        pins: [C_IN:2, U1:GND, C_OUT:2]
      - net: 3V3
        pins: [U1:VOUT, C_OUT:1]
"#;

const TEMPLATE_LED: &str = r#"# LED with Current Limiting Resistor
# Generated by kicaddy

meta:
  paper: A4
  title: "LED Circuit"
  author: "kicaddy"

components:
  R1:
    symbol: Device:R
    position: [100, 50]
    value: 330

  D1:
    symbol: Device:LED
    position: [100, 70]
    value: LED

connections:
  - net: VCC
    pins: [R1:1]
  - pins: [R1:2, D1:A]
  - net: GND
    pins: [D1:K]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_yaml() {
        let yaml = r#"
meta:
  paper: A4
  title: "Test"

components:
  R1:
    symbol: Device:R
    position: [100, 50]
    value: 10k
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        assert_eq!(schematic.meta.paper, "A4");
        assert_eq!(schematic.meta.title, Some("Test".to_string()));
        assert!(schematic.components.contains_key("R1"));
    }

    #[test]
    fn test_position_formats() {
        // Array format
        let yaml1 = r#"
components:
  R1:
    symbol: Device:R
    position: [100, 50]
"#;
        let sch1 = YamlSchematic::from_str(yaml1).unwrap();
        let r1 = sch1.components.get("R1").unwrap();
        let pos1 = r1.position.as_ref().unwrap();
        assert_eq!(pos1.x(), 100.0);
        assert_eq!(pos1.y(), 50.0);

        // Object format
        let yaml2 = r#"
components:
  R2:
    symbol: Device:R
    position:
      x: 200
      y: 75
"#;
        let sch2 = YamlSchematic::from_str(yaml2).unwrap();
        let r2 = sch2.components.get("R2").unwrap();
        let pos2 = r2.position.as_ref().unwrap();
        assert_eq!(pos2.x(), 200.0);
        assert_eq!(pos2.y(), 75.0);
    }

    #[test]
    fn test_position_optional() {
        // No position specified - should parse successfully
        let yaml = r#"
components:
  R1:
    symbol: Device:R
    value: 10k
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let r1 = schematic.components.get("R1").unwrap();
        assert!(r1.position.is_none());
    }

    #[test]
    fn test_groups_merge() {
        let yaml = r#"
groups:
  Power:
    components:
      C1:
        symbol: Device:C
        position: [50, 50]

components:
  R1:
    symbol: Device:R
    position: [100, 50]
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        let all = schematic.all_components();
        assert!(all.contains_key("R1"));
        assert!(all.contains_key("C1"));
    }

    #[test]
    fn test_connections() {
        let yaml = r#"
connections:
  - pins: [R1:1, C1:2]
  - net: GND
    pins: [R1:2, U1:GND]
    global: true
"#;
        let schematic = YamlSchematic::from_str(yaml).unwrap();
        assert_eq!(schematic.connections.len(), 2);
        assert_eq!(schematic.connections[0].net, None);
        assert_eq!(schematic.connections[1].net, Some("GND".to_string()));
        assert_eq!(schematic.connections[1].global, Some(true));
    }

    #[test]
    fn test_template_basic() {
        let content = YamlTemplate::Basic.content();
        let _schematic = YamlSchematic::from_str(content).unwrap();
    }
}
