//! Core data structures for KiCAD schematics

use serde::{Deserialize, Serialize};

use crate::common::{Color, Effects, Point, Position, Property, Stroke};
use crate::symbol::Symbol;

/// A complete schematic file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schematic {
    /// Format version (e.g., 20231120)
    pub version: u32,
    /// Generator that created the file
    pub generator: Option<String>,
    /// Generator version
    pub generator_version: Option<String>,
    /// Unique identifier
    pub uuid: String,
    /// Paper size
    pub paper: PaperSize,
    /// Title block information
    pub title_block: Option<TitleBlock>,
    /// Embedded symbol definitions used in this schematic
    pub lib_symbols: Vec<Symbol>,
    /// Wire/electrical junctions
    pub junctions: Vec<Junction>,
    /// No-connect markers
    pub no_connects: Vec<NoConnect>,
    /// Wire connections
    pub wires: Vec<Wire>,
    /// Bus connections
    pub buses: Vec<Bus>,
    /// Bus entry points
    pub bus_entries: Vec<BusEntry>,
    /// Global labels
    pub global_labels: Vec<GlobalLabel>,
    /// Hierarchical labels
    pub hierarchical_labels: Vec<HierarchicalLabel>,
    /// Local labels
    pub labels: Vec<Label>,
    /// Text annotations
    pub text_items: Vec<TextItem>,
    /// Placed symbol instances
    pub symbols: Vec<SymbolInstance>,
    /// Sheet instances (for hierarchical designs)
    pub sheet_instances: Vec<SheetInstance>,
    /// Whether to embed fonts
    pub embedded_fonts: bool,
}

impl Schematic {
    /// Current KiCAD schematic format version
    pub const CURRENT_VERSION: u32 = 20250114;

    /// Create a new empty schematic with default values
    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            generator: Some("kicaddy".to_string()),
            generator_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            uuid: uuid::Uuid::new_v4().to_string(),
            paper: PaperSize::A4,
            title_block: None,
            lib_symbols: Vec::new(),
            junctions: Vec::new(),
            no_connects: Vec::new(),
            wires: Vec::new(),
            buses: Vec::new(),
            bus_entries: Vec::new(),
            global_labels: Vec::new(),
            hierarchical_labels: Vec::new(),
            labels: Vec::new(),
            text_items: Vec::new(),
            symbols: Vec::new(),
            // Root sheet instance is required
            sheet_instances: vec![SheetInstance {
                path: "/".to_string(),
                page: "1".to_string(),
            }],
            embedded_fonts: false,
        }
    }

    /// Generate the next available reference for a given prefix
    ///
    /// # Arguments
    /// * `prefix` - Reference prefix (e.g., "R", "U", "#PWR")
    ///
    /// # Returns
    /// Next available reference (e.g., "R1", "U3", "#PWR05")
    pub fn next_reference(&self, prefix: &str) -> String {
        let mut max_num = 0u32;

        for symbol in &self.symbols {
            for prop in &symbol.properties {
                if prop.name == "Reference" {
                    // Check if this reference starts with our prefix
                    if prop.value.starts_with(prefix) {
                        // Extract the numeric suffix
                        let suffix = &prop.value[prefix.len()..];
                        if let Ok(num) = suffix.parse::<u32>() {
                            max_num = max_num.max(num);
                        }
                    }
                }
            }
        }

        // Use zero-padded format for power symbols (e.g., #PWR01)
        if prefix.starts_with('#') {
            format!("{}{:02}", prefix, max_num + 1)
        } else {
            format!("{}{}", prefix, max_num + 1)
        }
    }

    /// Add a symbol instance to the schematic
    ///
    /// This method:
    /// 1. Adds the symbol definition to lib_symbols if not already present
    /// 2. Creates and adds a SymbolInstance at the specified position
    ///
    /// # Arguments
    /// * `symbol` - The symbol definition from a library
    /// * `lib_id` - Library ID in the format "Library:Symbol" (e.g., "Device:R")
    /// * `position` - Position to place the symbol
    /// * `reference` - Optional reference designator (e.g., "R1"). If None, auto-generates unique one
    /// * `value` - Optional value. If None, uses symbol name
    ///
    /// # Returns
    /// The actual reference designator assigned to the symbol
    pub fn add_symbol(
        &mut self,
        symbol: &Symbol,
        lib_id: &str,
        position: Position,
        reference: Option<&str>,
        value: Option<&str>,
    ) -> String {
        // Add symbol to lib_symbols if not already present
        // lib_symbols uses the lib_id directly (e.g., "Device:R")
        if !self.lib_symbols.iter().any(|s| s.name == lib_id) {
            let mut lib_symbol = symbol.clone();
            lib_symbol.name = lib_id.to_string();
            // Unit names stay as original (e.g., "R_0_1", not "Device:R_0_1")
            self.lib_symbols.push(lib_symbol);
        }

        // Determine reference - auto-generate unique one if not provided or contains "?"
        let ref_value = match reference {
            Some(r) if !r.contains('?') => r.to_string(),
            Some(r) => {
                // Reference contains "?" - extract prefix and generate unique ref
                let prefix = r.trim_end_matches('?');
                self.next_reference(prefix)
            }
            None => {
                // No reference provided - use symbol's default prefix
                let prefix = symbol.reference().unwrap_or("U");
                self.next_reference(prefix)
            }
        };

        let val_value = value
            .map(|s| s.to_string())
            .unwrap_or_else(|| symbol.name.clone());

        // Create properties from symbol, updating Reference and Value
        // Filter out ki_* properties - those stay only in lib_symbols
        let mut properties: Vec<Property> = symbol
            .properties
            .iter()
            .filter(|p| !p.name.starts_with("ki_"))
            .map(|p| {
                let mut prop = p.clone();
                // Adjust property position relative to symbol position
                if let Some(ref mut pos) = prop.position {
                    pos.x += position.x;
                    pos.y += position.y;
                }
                // Override Reference and Value
                if prop.name == "Reference" {
                    prop.value = ref_value.clone();
                } else if prop.name == "Value" {
                    prop.value = val_value.clone();
                }
                prop
            })
            .collect();

        // Ensure we have at least Reference and Value properties
        if !properties.iter().any(|p| p.name == "Reference") {
            properties.insert(
                0,
                Property {
                    name: "Reference".to_string(),
                    value: ref_value.clone(),
                    position: Some(Position::new(position.x, position.y - 2.54, 0.0)),
                    effects: Some(Effects::default()),
                },
            );
        }
        if !properties.iter().any(|p| p.name == "Value") {
            properties.insert(
                1,
                Property {
                    name: "Value".to_string(),
                    value: val_value.clone(),
                    position: Some(Position::new(position.x, position.y + 2.54, 0.0)),
                    effects: Some(Effects::default()),
                },
            );
        }

        // Create pin instances for each unique pin in the symbol
        let mut pin_numbers: Vec<String> = symbol
            .units
            .iter()
            .flat_map(|u| u.pins.iter())
            .map(|p| p.number.number.clone())
            .collect();
        pin_numbers.sort();
        pin_numbers.dedup();

        let pins: Vec<PinInstance> = pin_numbers
            .into_iter()
            .map(|number| PinInstance {
                number,
                uuid: uuid::Uuid::new_v4().to_string(),
            })
            .collect();

        // Create project instance with reference
        // The path is "/{schematic_uuid}" for the root sheet
        let project_instance = ProjectInstance {
            project_name: String::new(), // Empty project name for standalone schematics
            paths: vec![PathInstance {
                path: format!("/{}", self.uuid),
                reference: ref_value.clone(),
                unit: 1,
            }],
        };

        // Create the symbol instance
        let instance = SymbolInstance {
            lib_id: lib_id.to_string(),
            position,
            unit: 1,
            exclude_from_sim: symbol.exclude_from_sim,
            in_bom: symbol.in_bom,
            on_board: symbol.on_board,
            dnp: false,
            fields_autoplaced: true,
            uuid: uuid::Uuid::new_v4().to_string(),
            properties,
            pins,
            instances: vec![project_instance],
            mirror: None,
        };

        self.symbols.push(instance);

        ref_value
    }

    /// Find symbol by reference designator (e.g., "R1", "U1")
    pub fn find_symbol_by_reference(&self, reference: &str) -> Option<&SymbolInstance> {
        self.symbols.iter().find(|s| {
            s.properties
                .iter()
                .any(|p| p.name == "Reference" && p.value == reference)
        })
    }

    /// Get pin world position from symbol instance
    /// Returns (Point, pin_angle) - Point is where wire connects
    pub fn get_pin_position(&self, symbol: &SymbolInstance, pin: &str) -> Option<(Point, f64)> {
        // Find the lib_symbol by lib_id
        let lib_symbol = self.lib_symbols.iter().find(|s| s.name == symbol.lib_id)?;

        // Find the pin in the symbol units (check both pin number and name)
        let lib_pin = lib_symbol
            .units
            .iter()
            .flat_map(|u| u.pins.iter())
            .find(|p| p.number.number == pin || p.name.name == pin)?;

        // Get pin's local position and angle
        let pin_pos = lib_pin.position;
        let pin_length = lib_pin.length;

        // Calculate pin direction vector based on pin angle
        // Pin angle 0 = pointing right, 90 = up, 180 = left, 270 = down
        let pin_angle_rad = pin_pos.angle.to_radians();

        // The pin's body starts at pin_pos and extends in the direction of pin_angle
        // Wire attachment point is at the end of the pin (away from symbol body)
        // The tip (wire attachment) is at pin_pos - (pin_length in pin direction)
        let tip_local_x = pin_pos.x - pin_length * pin_angle_rad.cos();
        let tip_local_y = pin_pos.y + pin_length * pin_angle_rad.sin();

        // Apply mirror transformation if set
        let (mirrored_x, mirrored_y, angle_adjust) = match symbol.mirror {
            Some(Mirror::X) => (tip_local_x, -tip_local_y, -1.0),
            Some(Mirror::Y) => (-tip_local_x, tip_local_y, -1.0),
            None => (tip_local_x, tip_local_y, 1.0),
        };

        // Apply symbol rotation
        let sym_angle_rad = symbol.position.angle.to_radians();
        let cos_a = sym_angle_rad.cos();
        let sin_a = sym_angle_rad.sin();

        let rotated_x = mirrored_x * cos_a - mirrored_y * sin_a;
        let rotated_y = mirrored_x * sin_a + mirrored_y * cos_a;

        // Translate by symbol position
        let world_x = rotated_x + symbol.position.x;
        let world_y = rotated_y + symbol.position.y;

        // Calculate final pin angle in world coordinates
        let world_pin_angle = (pin_pos.angle + symbol.position.angle * angle_adjust) % 360.0;

        Some((Point::new(world_x, world_y), world_pin_angle))
    }

    /// Add wire between two points (auto-snaps to grid)
    pub fn add_wire(&mut self, from: Point, to: Point) {
        const GRID: f64 = 1.27;
        let snapped_from = Point::new(
            (from.x / GRID).round() * GRID,
            (from.y / GRID).round() * GRID,
        );
        let snapped_to = Point::new(
            (to.x / GRID).round() * GRID,
            (to.y / GRID).round() * GRID,
        );

        let wire = Wire {
            points: vec![snapped_from, snapped_to],
            stroke: Stroke::default(),
            uuid: uuid::Uuid::new_v4().to_string(),
        };
        self.wires.push(wire);
    }

    /// Add wire with routing mode
    pub fn add_wire_routed(&mut self, from: Point, to: Point, mode: RoutingMode) {
        const GRID: f64 = 1.27;
        let snapped_from = Point::new(
            (from.x / GRID).round() * GRID,
            (from.y / GRID).round() * GRID,
        );
        let snapped_to = Point::new(
            (to.x / GRID).round() * GRID,
            (to.y / GRID).round() * GRID,
        );

        let points = match mode {
            RoutingMode::Direct => vec![snapped_from, snapped_to],
            RoutingMode::Orthogonal => {
                // Horizontal then vertical
                let mid = Point::new(snapped_to.x, snapped_from.y);
                if (mid.x - snapped_from.x).abs() < 0.001 || (mid.y - snapped_to.y).abs() < 0.001 {
                    // Already aligned, single segment
                    vec![snapped_from, snapped_to]
                } else {
                    vec![snapped_from, mid, snapped_to]
                }
            }
            RoutingMode::OrthogonalVH => {
                // Vertical then horizontal
                let mid = Point::new(snapped_from.x, snapped_to.y);
                if (mid.y - snapped_from.y).abs() < 0.001 || (mid.x - snapped_to.x).abs() < 0.001 {
                    // Already aligned, single segment
                    vec![snapped_from, snapped_to]
                } else {
                    vec![snapped_from, mid, snapped_to]
                }
            }
        };

        let wire = Wire {
            points,
            stroke: Stroke::default(),
            uuid: uuid::Uuid::new_v4().to_string(),
        };
        self.wires.push(wire);
    }

    /// Add junction at point
    pub fn add_junction(&mut self, position: Point) {
        const GRID: f64 = 1.27;
        let snapped = Point::new(
            (position.x / GRID).round() * GRID,
            (position.y / GRID).round() * GRID,
        );

        let junction = Junction {
            position: snapped,
            diameter: 0.0,
            color: None,
            uuid: uuid::Uuid::new_v4().to_string(),
        };
        self.junctions.push(junction);
    }

    /// Add local label
    pub fn add_label(&mut self, text: &str, position: Position) {
        const GRID: f64 = 1.27;
        let snapped_pos = Position::new(
            (position.x / GRID).round() * GRID,
            (position.y / GRID).round() * GRID,
            position.angle,
        );

        let label = Label {
            text: text.to_string(),
            position: snapped_pos,
            fields_autoplaced: true,
            effects: Some(Effects::default()),
            uuid: uuid::Uuid::new_v4().to_string(),
        };
        self.labels.push(label);
    }

    /// Add global label with shape
    pub fn add_global_label(&mut self, text: &str, position: Position, shape: LabelShape) {
        const GRID: f64 = 1.27;
        let snapped_pos = Position::new(
            (position.x / GRID).round() * GRID,
            (position.y / GRID).round() * GRID,
            position.angle,
        );

        let label = GlobalLabel {
            text: text.to_string(),
            shape,
            position: snapped_pos,
            fields_autoplaced: true,
            effects: Some(Effects::default()),
            uuid: uuid::Uuid::new_v4().to_string(),
            properties: vec![],
        };
        self.global_labels.push(label);
    }

    /// Get all pin positions for a symbol instance
    /// Returns Vec of (Point, pin_angle) for each pin
    pub fn get_all_pin_positions(&self, symbol: &SymbolInstance) -> Vec<(Point, f64)> {
        let lib_symbol = match self.lib_symbols.iter().find(|s| s.name == symbol.lib_id) {
            Some(s) => s,
            None => return vec![],
        };

        lib_symbol
            .units
            .iter()
            .flat_map(|u| u.pins.iter())
            .filter_map(|pin| {
                let pin_pos = pin.position;
                let pin_length = pin.length;
                let pin_angle_rad = pin_pos.angle.to_radians();

                let tip_local_x = pin_pos.x - pin_length * pin_angle_rad.cos();
                let tip_local_y = pin_pos.y + pin_length * pin_angle_rad.sin();

                let (mirrored_x, mirrored_y, angle_adjust) = match symbol.mirror {
                    Some(Mirror::X) => (tip_local_x, -tip_local_y, -1.0),
                    Some(Mirror::Y) => (-tip_local_x, tip_local_y, -1.0),
                    None => (tip_local_x, tip_local_y, 1.0),
                };

                let sym_angle_rad = symbol.position.angle.to_radians();
                let cos_a = sym_angle_rad.cos();
                let sin_a = sym_angle_rad.sin();

                let rotated_x = mirrored_x * cos_a - mirrored_y * sin_a;
                let rotated_y = mirrored_x * sin_a + mirrored_y * cos_a;

                let world_x = rotated_x + symbol.position.x;
                let world_y = rotated_y + symbol.position.y;
                let world_pin_angle = (pin_pos.angle + symbol.position.angle * angle_adjust) % 360.0;

                Some((Point::new(world_x, world_y), world_pin_angle))
            })
            .collect()
    }

    /// Delete a symbol by reference designator
    /// Also deletes: labels at pin positions, wires with endpoints at pin positions,
    /// and removes lib_symbol if no other instances use it
    pub fn delete_symbol(&mut self, reference: &str) -> bool {
        // Find the symbol index
        let symbol_idx = self.symbols.iter().position(|s| {
            s.properties
                .iter()
                .any(|p| p.name == "Reference" && p.value == reference)
        });

        let symbol_idx = match symbol_idx {
            Some(idx) => idx,
            None => return false,
        };

        // Get pin positions before removing the symbol
        let pin_positions: Vec<Point> = self
            .get_all_pin_positions(&self.symbols[symbol_idx])
            .into_iter()
            .map(|(p, _)| p)
            .collect();

        let lib_id = self.symbols[symbol_idx].lib_id.clone();

        // Remove the symbol
        self.symbols.remove(symbol_idx);

        // Check if any other symbol uses the same lib_id
        let lib_id_still_used = self.symbols.iter().any(|s| s.lib_id == lib_id);
        if !lib_id_still_used {
            self.lib_symbols.retain(|s| s.name != lib_id);
        }

        // Delete labels at pin positions (with tolerance for floating point)
        const TOLERANCE: f64 = 0.1;
        self.labels.retain(|label| {
            !pin_positions.iter().any(|pin_pos| {
                (label.position.x - pin_pos.x).abs() < TOLERANCE
                    && (label.position.y - pin_pos.y).abs() < TOLERANCE
            })
        });

        self.global_labels.retain(|label| {
            !pin_positions.iter().any(|pin_pos| {
                (label.position.x - pin_pos.x).abs() < TOLERANCE
                    && (label.position.y - pin_pos.y).abs() < TOLERANCE
            })
        });

        // Delete wires with endpoints at pin positions
        self.wires.retain(|wire| {
            if wire.points.is_empty() {
                return true;
            }
            let first = &wire.points[0];
            let last = &wire.points[wire.points.len() - 1];

            !pin_positions.iter().any(|pin_pos| {
                ((first.x - pin_pos.x).abs() < TOLERANCE
                    && (first.y - pin_pos.y).abs() < TOLERANCE)
                    || ((last.x - pin_pos.x).abs() < TOLERANCE
                        && (last.y - pin_pos.y).abs() < TOLERANCE)
            })
        });

        true
    }

    /// Delete wire at or near a point
    /// Returns true if a wire was deleted
    pub fn delete_wire_at(&mut self, point: Point) -> bool {
        const TOLERANCE: f64 = 1.27; // One grid unit tolerance

        let wire_idx = self.wires.iter().position(|wire| {
            // Check if point is near any segment of the wire
            for i in 0..wire.points.len().saturating_sub(1) {
                let p1 = &wire.points[i];
                let p2 = &wire.points[i + 1];
                if point_near_segment(point, *p1, *p2, TOLERANCE) {
                    return true;
                }
            }
            false
        });

        if let Some(idx) = wire_idx {
            self.wires.remove(idx);
            true
        } else {
            false
        }
    }

    /// Delete label by name, optionally at specific position
    /// Returns true if a label was deleted
    pub fn delete_label(&mut self, name: &str, position: Option<Point>) -> bool {
        const TOLERANCE: f64 = 0.1;

        let label_idx = self.labels.iter().position(|label| {
            if label.text != name {
                return false;
            }
            if let Some(pos) = position {
                (label.position.x - pos.x).abs() < TOLERANCE
                    && (label.position.y - pos.y).abs() < TOLERANCE
            } else {
                true
            }
        });

        if let Some(idx) = label_idx {
            self.labels.remove(idx);
            return true;
        }

        // Try global labels
        let global_idx = self.global_labels.iter().position(|label| {
            if label.text != name {
                return false;
            }
            if let Some(pos) = position {
                (label.position.x - pos.x).abs() < TOLERANCE
                    && (label.position.y - pos.y).abs() < TOLERANCE
            } else {
                true
            }
        });

        if let Some(idx) = global_idx {
            self.global_labels.remove(idx);
            return true;
        }

        false
    }
}

/// Check if a point is near a line segment
fn point_near_segment(point: Point, p1: Point, p2: Point, tolerance: f64) -> bool {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.0001 {
        // Segment is essentially a point
        let dist = ((point.x - p1.x).powi(2) + (point.y - p1.y).powi(2)).sqrt();
        return dist < tolerance;
    }

    // Project point onto line, clamped to segment
    let t = ((point.x - p1.x) * dx + (point.y - p1.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let closest_x = p1.x + t * dx;
    let closest_y = p1.y + t * dy;

    let dist = ((point.x - closest_x).powi(2) + (point.y - closest_y).powi(2)).sqrt();
    dist < tolerance
}

impl Default for Schematic {
    fn default() -> Self {
        Self::new()
    }
}

/// Paper size for the schematic
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaperSize {
    A4,
    A3,
    A2,
    A1,
    A0,
    A,
    B,
    C,
    D,
    E,
    USLetter,
    USLegal,
    USLedger,
    Custom { width: f64, height: f64 },
}

impl PaperSize {
    pub fn from_str(s: &str) -> Self {
        match s {
            "A4" => PaperSize::A4,
            "A3" => PaperSize::A3,
            "A2" => PaperSize::A2,
            "A1" => PaperSize::A1,
            "A0" => PaperSize::A0,
            "A" => PaperSize::A,
            "B" => PaperSize::B,
            "C" => PaperSize::C,
            "D" => PaperSize::D,
            "E" => PaperSize::E,
            "USLetter" => PaperSize::USLetter,
            "USLegal" => PaperSize::USLegal,
            "USLedger" => PaperSize::USLedger,
            _ => PaperSize::A4, // Default
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PaperSize::A4 => "A4",
            PaperSize::A3 => "A3",
            PaperSize::A2 => "A2",
            PaperSize::A1 => "A1",
            PaperSize::A0 => "A0",
            PaperSize::A => "A",
            PaperSize::B => "B",
            PaperSize::C => "C",
            PaperSize::D => "D",
            PaperSize::E => "E",
            PaperSize::USLetter => "USLetter",
            PaperSize::USLegal => "USLegal",
            PaperSize::USLedger => "USLedger",
            PaperSize::Custom { .. } => "User",
        }
    }
}

impl Default for PaperSize {
    fn default() -> Self {
        PaperSize::A4
    }
}

/// Title block information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TitleBlock {
    pub title: Option<String>,
    pub date: Option<String>,
    pub rev: Option<String>,
    pub company: Option<String>,
    /// Comment fields (indexed 1-9)
    pub comments: Vec<(u8, String)>,
}

/// A junction (connection point) in the schematic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Junction {
    pub position: Point,
    pub diameter: f64,
    pub color: Option<Color>,
    pub uuid: String,
}

/// A no-connect marker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoConnect {
    pub position: Point,
    pub uuid: String,
}

/// A wire connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wire {
    pub points: Vec<Point>,
    pub stroke: Stroke,
    pub uuid: String,
}

/// A bus connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bus {
    pub points: Vec<Point>,
    pub stroke: Stroke,
    pub uuid: String,
}

/// A bus entry point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusEntry {
    pub position: Point,
    pub size: Point,
    pub stroke: Stroke,
    pub uuid: String,
}

/// A text annotation in the schematic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub text: String,
    pub position: Position,
    pub effects: Option<Effects>,
    pub uuid: String,
}

/// A global label (visible across all sheets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalLabel {
    pub text: String,
    pub shape: LabelShape,
    pub position: Position,
    pub fields_autoplaced: bool,
    pub effects: Option<Effects>,
    pub uuid: String,
    pub properties: Vec<Property>,
}

/// A hierarchical label (for sheet connections)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchicalLabel {
    pub text: String,
    pub shape: LabelShape,
    pub position: Position,
    pub fields_autoplaced: bool,
    pub effects: Option<Effects>,
    pub uuid: String,
    pub properties: Vec<Property>,
}

/// A local label (net name)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    pub position: Position,
    pub fields_autoplaced: bool,
    pub effects: Option<Effects>,
    pub uuid: String,
}

/// Label shape for global and hierarchical labels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum LabelShape {
    #[default]
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
}

impl LabelShape {
    pub fn from_str(s: &str) -> Self {
        match s {
            "input" => LabelShape::Input,
            "output" => LabelShape::Output,
            "bidirectional" => LabelShape::Bidirectional,
            "tri_state" => LabelShape::TriState,
            "passive" => LabelShape::Passive,
            _ => LabelShape::Input,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LabelShape::Input => "input",
            LabelShape::Output => "output",
            LabelShape::Bidirectional => "bidirectional",
            LabelShape::TriState => "tri_state",
            LabelShape::Passive => "passive",
        }
    }
}

/// A placed symbol instance in the schematic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInstance {
    /// Library ID (e.g., "Device:R")
    pub lib_id: String,
    /// Position in the schematic
    pub position: Position,
    /// Unit number (for multi-unit symbols)
    pub unit: u32,
    /// Exclude from simulation
    pub exclude_from_sim: bool,
    /// Include in BOM
    pub in_bom: bool,
    /// Include on board
    pub on_board: bool,
    /// Do Not Populate flag
    pub dnp: bool,
    /// Whether fields are auto-placed
    pub fields_autoplaced: bool,
    /// Unique identifier
    pub uuid: String,
    /// Instance properties (Reference, Value, etc.)
    pub properties: Vec<Property>,
    /// Pin instances
    pub pins: Vec<PinInstance>,
    /// Instance data for hierarchical designs
    pub instances: Vec<ProjectInstance>,
    /// Mirror setting
    pub mirror: Option<Mirror>,
}

/// Pin instance in a symbol placement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinInstance {
    pub number: String,
    pub uuid: String,
}

/// Project instance data for hierarchical designs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInstance {
    pub project_name: String,
    pub paths: Vec<PathInstance>,
}

/// Path instance within a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathInstance {
    pub path: String,
    pub reference: String,
    pub unit: u32,
}

/// Mirror setting for symbol instances
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Mirror {
    X,
    Y,
}

impl Mirror {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "x" => Some(Mirror::X),
            "y" => Some(Mirror::Y),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Mirror::X => "x",
            Mirror::Y => "y",
        }
    }
}

/// Sheet instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetInstance {
    pub path: String,
    pub page: String,
}

/// Routing mode for wire placement
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RoutingMode {
    /// Single segment, diagonal allowed
    #[default]
    Direct,
    /// Horizontal then vertical (two segments)
    Orthogonal,
    /// Vertical then horizontal
    OrthogonalVH,
}
