//! Outline tool - Get tree view of schematic with components, pins, and nets

use std::collections::HashMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{load_schematic, Tool, ToolError};
use crate::common::Point;
use crate::schematic::{Mirror, PaperSize, Schematic, SymbolInstance};
use crate::symbol::graphics::GraphicItem;
use crate::symbol::Symbol;

/// Input for outline tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutlineInput {
    /// Path to the schematic file
    pub schematic: PathBuf,
}

/// Output of outline tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineOutput {
    /// Components in the schematic
    pub components: Vec<ComponentInfo>,
    /// Nets connecting components
    pub nets: Vec<NetInfo>,
    /// Summary statistics
    pub stats: SchematicStats,
    /// Paper size name (e.g., "A4", "USLetter")
    pub paper: String,
    /// Paper width in mils
    pub paper_width: f64,
    /// Paper height in mils
    pub paper_height: f64,
}

/// Bounding box in schematic coordinates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BoundingBox {
    /// Create a new empty bounding box
    fn new() -> Self {
        Self {
            min_x: f64::MAX,
            min_y: f64::MAX,
            max_x: f64::MIN,
            max_y: f64::MIN,
        }
    }

    /// Expand the bounding box to include a point
    fn include_point(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    /// Check if the bounding box is valid (has been expanded)
    fn is_valid(&self) -> bool {
        self.min_x <= self.max_x && self.min_y <= self.max_y
    }
}

/// Information about a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Reference designator (e.g., "R1")
    pub reference: String,
    /// Library ID (e.g., "Device:R")
    pub lib_id: String,
    /// Component value (e.g., "10k")
    pub value: String,
    /// Position in schematic
    pub x: f64,
    pub y: f64,
    /// Rotation angle in degrees
    pub angle: f64,
    /// Pins with connectivity info
    pub pins: Vec<PinInfo>,
    /// Bounding box in schematic coordinates
    pub bounds: BoundingBox,
}

/// Information about a pin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinInfo {
    /// Pin number
    pub number: String,
    /// Pin name (if different from number)
    pub name: String,
    /// Net label this pin is connected to (if any)
    pub net: Option<String>,
    /// Other pins directly connected to this pin (as "REF:PIN" strings)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub connected_to: Vec<String>,
    /// Whether this pin is unconnected
    pub unconnected: bool,
    /// Pin position in schematic coordinates (for backward compatibility)
    pub x: f64,
    pub y: f64,
}

/// Net label scope - determines the visibility/reach of a net
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NetLabelScope {
    /// Global scope - visible across all sheets (from global_label or power symbols)
    #[default]
    Global,
    /// Hierarchical scope - connects parent/child sheets (from hierarchical_label)
    Hierarchical,
    /// Local scope - only visible within current sheet (from label)
    Local,
}

/// Information about a net
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetInfo {
    /// Net name
    pub name: String,
    /// Whether this is a global net (from global label) - kept for backward compatibility
    pub is_global: bool,
    /// Net label scope (Global, Hierarchical, or Local)
    pub scope: NetLabelScope,
    /// Connections in "REF:PIN" format
    pub connections: Vec<String>,
}

/// Summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicStats {
    pub component_count: usize,
    pub wire_count: usize,
    pub junction_count: usize,
    pub label_count: usize,
    pub global_label_count: usize,
    pub net_count: usize,
}

/// Outline tool implementation
pub struct OutlineTool;

impl Tool for OutlineTool {
    const NAME: &'static str = "outline";
    const DESCRIPTION: &'static str = "Get complete schematic state: all components with pin positions and bounding boxes, nets with connectivity, and paper dimensions. **Use this to understand the schematic before making changes.**";

    type Input = OutlineInput;
    type Output = OutlineOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let schematic = load_schematic(&input.schematic)?;
        Ok(build_outline(&schematic))
    }
}

/// Build the outline from a schematic
pub fn build_outline(schematic: &Schematic) -> OutlineOutput {
    // Step 1: Collect all connection points
    let mut connection_points: Vec<ConnectionPoint> = Vec::new();

    // Add pin positions and power symbols
    for symbol in &schematic.symbols {
        let reference = symbol
            .properties
            .iter()
            .find(|p| p.name == "Reference")
            .map(|p| p.value.clone())
            .unwrap_or_else(|| "?".to_string());

        // Power symbols (reference starts with #) create implicit global nets
        // Their Value property becomes the net name (e.g., GND, +5V)
        if reference.starts_with('#') {
            let net_name = symbol
                .properties
                .iter()
                .find(|p| p.name == "Value")
                .map(|p| p.value.clone())
                .unwrap_or_default();

            // Add the power symbol's pin as a global label
            for pin in &symbol.pins {
                if let Some((pos, _angle)) = schematic.get_pin_position(symbol, &pin.number) {
                    connection_points.push(ConnectionPoint {
                        position: pos,
                        kind: ConnectionKind::Label {
                            name: net_name.clone(),
                            scope: NetLabelScope::Global,
                        },
                    });
                }
            }
            continue;
        }

        // Get lib_symbol for pin names
        let lib_symbol = schematic.lib_symbols.iter().find(|s| s.name == symbol.lib_id);

        for pin in &symbol.pins {
            if let Some((pos, _angle)) = schematic.get_pin_position(symbol, &pin.number) {
                // Get pin name from lib_symbol
                let pin_name = lib_symbol
                    .and_then(|s| {
                        s.units
                            .iter()
                            .flat_map(|u| u.pins.iter())
                            .find(|p| p.number.number == pin.number)
                            .map(|p| p.name.name.clone())
                    })
                    .unwrap_or_else(|| pin.number.clone());

                connection_points.push(ConnectionPoint {
                    position: pos,
                    kind: ConnectionKind::Pin {
                        reference: reference.clone(),
                        pin_number: pin.number.clone(),
                        pin_name,
                    },
                });
            }
        }
    }

    // Add wire endpoints
    for wire in &schematic.wires {
        if !wire.points.is_empty() {
            connection_points.push(ConnectionPoint {
                position: wire.points[0],
                kind: ConnectionKind::WireEndpoint,
            });
            if wire.points.len() > 1 {
                connection_points.push(ConnectionPoint {
                    position: wire.points[wire.points.len() - 1],
                    kind: ConnectionKind::WireEndpoint,
                });
            }
            // Add intermediate points as well (for multi-segment wires)
            for point in wire.points.iter().skip(1).take(wire.points.len().saturating_sub(2)) {
                connection_points.push(ConnectionPoint {
                    position: *point,
                    kind: ConnectionKind::WireEndpoint,
                });
            }
        }
    }

    // Add junctions
    for junction in &schematic.junctions {
        connection_points.push(ConnectionPoint {
            position: junction.position,
            kind: ConnectionKind::Junction,
        });
    }

    // Add local labels
    for label in &schematic.labels {
        connection_points.push(ConnectionPoint {
            position: Point::new(label.position.x, label.position.y),
            kind: ConnectionKind::Label {
                name: label.text.clone(),
                scope: NetLabelScope::Local,
            },
        });
    }

    // Add global labels
    for label in &schematic.global_labels {
        connection_points.push(ConnectionPoint {
            position: Point::new(label.position.x, label.position.y),
            kind: ConnectionKind::Label {
                name: label.text.clone(),
                scope: NetLabelScope::Global,
            },
        });
    }

    // Add hierarchical labels
    for label in &schematic.hierarchical_labels {
        connection_points.push(ConnectionPoint {
            position: Point::new(label.position.x, label.position.y),
            kind: ConnectionKind::Label {
                name: label.text.clone(),
                scope: NetLabelScope::Hierarchical,
            },
        });
    }

    // Add sheet pins
    for sheet in &schematic.sheets {
        for pin in &sheet.pins {
            connection_points.push(ConnectionPoint {
                position: Point::new(pin.position.x, pin.position.y),
                kind: ConnectionKind::SheetPin {
                    sheet_name: sheet.sheet_name.clone(),
                    pin_name: pin.name.clone(),
                },
            });
        }
    }

    // Step 2: Build connectivity using union-find
    let mut uf = UnionFind::new(connection_points.len());

    // Connect points that are at the same position (with tolerance)
    const TOLERANCE: f64 = 0.5;
    for i in 0..connection_points.len() {
        for j in (i + 1)..connection_points.len() {
            let dist_sq = {
                let dx = connection_points[i].position.x - connection_points[j].position.x;
                let dy = connection_points[i].position.y - connection_points[j].position.y;
                dx * dx + dy * dy
            };
            if dist_sq < TOLERANCE * TOLERANCE {
                uf.union(i, j);
            }
        }
    }

    // Connect wire segments
    for wire in &schematic.wires {
        for i in 0..wire.points.len().saturating_sub(1) {
            let p1 = wire.points[i];
            let p2 = wire.points[i + 1];

            // Find indices of these points in connection_points
            let idx1 = connection_points.iter().position(|cp| {
                let dx = cp.position.x - p1.x;
                let dy = cp.position.y - p1.y;
                dx * dx + dy * dy < TOLERANCE * TOLERANCE
            });

            let idx2 = connection_points.iter().position(|cp| {
                let dx = cp.position.x - p2.x;
                let dy = cp.position.y - p2.y;
                dx * dx + dy * dy < TOLERANCE * TOLERANCE
            });

            if let (Some(i1), Some(i2)) = (idx1, idx2) {
                uf.union(i1, i2);
            }
        }
    }

    // Step 3: Group connection points by their root (net)
    let mut net_groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..connection_points.len() {
        let root = uf.find(i);
        net_groups.entry(root).or_default().push(i);
    }

    // Step 4: Assign net names and build output
    let mut nets: Vec<NetInfo> = Vec::new();
    let mut pin_to_net: HashMap<(String, String), String> = HashMap::new();
    let mut auto_net_counter = 1;

    for (_root, indices) in &net_groups {
        // Find pins, sheet pins, and labels in this group
        let mut pins: Vec<(String, String)> = Vec::new();
        let mut sheet_pins: Vec<(String, String)> = Vec::new(); // (sheet_name, pin_name)
        let mut label_info: Option<(String, NetLabelScope)> = None;

        for &idx in indices {
            match &connection_points[idx].kind {
                ConnectionKind::Pin {
                    reference,
                    pin_number,
                    ..
                } => {
                    pins.push((reference.clone(), pin_number.clone()));
                }
                ConnectionKind::SheetPin {
                    sheet_name,
                    pin_name,
                } => {
                    sheet_pins.push((sheet_name.clone(), pin_name.clone()));
                }
                ConnectionKind::Label { name, scope } => {
                    // Prefer labels with higher scope: Global > Hierarchical > Local
                    let should_update = match (&label_info, scope) {
                        (None, _) => true,
                        (Some((_, NetLabelScope::Local)), NetLabelScope::Hierarchical | NetLabelScope::Global) => true,
                        (Some((_, NetLabelScope::Hierarchical)), NetLabelScope::Global) => true,
                        _ => false,
                    };
                    if should_update {
                        label_info = Some((name.clone(), *scope));
                    }
                }
                _ => {}
            }
        }

        // Only create net if there are pins or sheet pins connected
        if pins.is_empty() && sheet_pins.is_empty() {
            continue;
        }

        // Determine net name and scope
        let (net_name, scope) = match label_info {
            Some((name, scope)) => (name, scope),
            None => {
                // Auto-generate net name - local scope
                let name = format!("NET_{}", auto_net_counter);
                auto_net_counter += 1;
                (name, NetLabelScope::Local)
            }
        };
        let is_global = scope == NetLabelScope::Global;

        // Map pins to net
        for (ref_, pin) in &pins {
            pin_to_net.insert((ref_.clone(), pin.clone()), net_name.clone());
        }

        // Build connections list (component pins + sheet pins)
        let mut connections: Vec<String> = pins.iter().map(|(r, p)| format!("{}:{}", r, p)).collect();
        for (sheet, pin) in &sheet_pins {
            connections.push(format!("{}:{}", sheet, pin));
        }

        nets.push(NetInfo {
            name: net_name,
            is_global,
            scope,
            connections,
        });
    }

    // Step 5: Build component info
    let mut components: Vec<ComponentInfo> = Vec::new();

    for symbol in &schematic.symbols {
        let reference = symbol
            .properties
            .iter()
            .find(|p| p.name == "Reference")
            .map(|p| p.value.clone())
            .unwrap_or_else(|| "?".to_string());

        // Skip power symbols (they're handled as implicit global labels)
        if reference.starts_with('#') {
            continue;
        }

        let value = symbol
            .properties
            .iter()
            .find(|p| p.name == "Value")
            .map(|p| p.value.clone())
            .unwrap_or_default();

        // Get lib_symbol for pin names
        let lib_symbol = schematic.lib_symbols.iter().find(|s| s.name == symbol.lib_id);

        let mut pins: Vec<PinInfo> = Vec::new();
        for pin in &symbol.pins {
            let (pos, _angle) = schematic
                .get_pin_position(symbol, &pin.number)
                .unwrap_or((Point::new(0.0, 0.0), 0.0));

            // Get pin name from lib_symbol
            let pin_name = lib_symbol
                .and_then(|s| {
                    s.units
                        .iter()
                        .flat_map(|u| u.pins.iter())
                        .find(|p| p.number.number == pin.number)
                        .map(|p| p.name.name.clone())
                })
                .unwrap_or_else(|| pin.number.clone());

            let net = pin_to_net
                .get(&(reference.clone(), pin.number.clone()))
                .cloned();

            // Find other pins connected to this pin (excluding those with same net label)
            let connected_to: Vec<String> = if net.is_none() {
                // Only show connected_to when there's no net label
                // Find all other pins in the same net group
                let mut connected = Vec::new();
                for (_root, indices) in &net_groups {
                    let this_pin_in_group = indices.iter().any(|&idx| {
                        matches!(&connection_points[idx].kind, ConnectionKind::Pin { reference: r, pin_number: p, .. }
                            if r == &reference && p == &pin.number)
                    });

                    if this_pin_in_group {
                        for &idx in indices {
                            if let ConnectionKind::Pin { reference: other_ref, pin_number: other_pin, .. } = &connection_points[idx].kind {
                                if other_ref != &reference || other_pin != &pin.number {
                                    connected.push(format!("{}:{}", other_ref, other_pin));
                                }
                            }
                        }
                        break;
                    }
                }
                connected.sort();
                connected
            } else {
                Vec::new()
            };

            // Determine if pin is unconnected
            let unconnected = net.is_none() && connected_to.is_empty();

            pins.push(PinInfo {
                number: pin.number.clone(),
                name: pin_name,
                net,
                connected_to,
                unconnected,
                x: pos.x,
                y: pos.y,
            });
        }

        // Calculate bounding box
        let bounds = calculate_bounds(symbol, lib_symbol, schematic);

        components.push(ComponentInfo {
            reference,
            lib_id: symbol.lib_id.clone(),
            value,
            x: symbol.position.x,
            y: symbol.position.y,
            angle: symbol.position.angle,
            pins,
            bounds,
        });
    }

    // Sort components by reference for consistent output
    components.sort_by(|a, b| {
        // Extract prefix and number from reference
        let parse_ref = |r: &str| -> (String, i32) {
            let prefix: String = r.chars().take_while(|c| c.is_alphabetic()).collect();
            let num: i32 = r
                .chars()
                .skip_while(|c| c.is_alphabetic())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            (prefix, num)
        };
        let (ap, an) = parse_ref(&a.reference);
        let (bp, bn) = parse_ref(&b.reference);
        (ap, an).cmp(&(bp, bn))
    });

    // Sort nets by name
    nets.sort_by(|a, b| a.name.cmp(&b.name));

    let stats = SchematicStats {
        component_count: schematic.symbols.len(),
        wire_count: schematic.wires.len(),
        junction_count: schematic.junctions.len(),
        label_count: schematic.labels.len(),
        global_label_count: schematic.global_labels.len(),
        net_count: nets.len(),
    };

    // Get paper dimensions
    let (paper_width, paper_height) = paper_dimensions(&schematic.paper);

    OutlineOutput {
        components,
        nets,
        stats,
        paper: schematic.paper.as_str().to_string(),
        paper_width,
        paper_height,
    }
}

/// A connection point in the schematic
#[derive(Debug, Clone)]
struct ConnectionPoint {
    position: Point,
    kind: ConnectionKind,
}

/// Kind of connection point
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ConnectionKind {
    Pin {
        reference: String,
        pin_number: String,
        pin_name: String, // Kept for potential future use (e.g., named net assignment)
    },
    SheetPin {
        sheet_name: String,
        pin_name: String,
    },
    WireEndpoint,
    Junction,
    Label {
        name: String,
        scope: NetLabelScope,
    },
}

/// Simple union-find data structure for connectivity
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]); // Path compression
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx != ry {
            // Union by rank
            match self.rank[rx].cmp(&self.rank[ry]) {
                std::cmp::Ordering::Less => self.parent[rx] = ry,
                std::cmp::Ordering::Greater => self.parent[ry] = rx,
                std::cmp::Ordering::Equal => {
                    self.parent[ry] = rx;
                    self.rank[rx] += 1;
                }
            }
        }
    }
}

/// Get paper dimensions in mils (width, height)
fn paper_dimensions(paper: &PaperSize) -> (f64, f64) {
    // Dimensions in mils (1 mil = 0.0254mm)
    // KiCAD uses mm internally, but for user convenience we report in mils
    // which is the unit used in position coordinates
    match paper {
        PaperSize::A4 => (210.0, 297.0),        // 210x297 mm
        PaperSize::A3 => (297.0, 420.0),        // 297x420 mm
        PaperSize::A2 => (420.0, 594.0),        // 420x594 mm
        PaperSize::A1 => (594.0, 841.0),        // 594x841 mm
        PaperSize::A0 => (841.0, 1189.0),       // 841x1189 mm
        PaperSize::A => (215.9, 279.4),         // 8.5x11 inch
        PaperSize::B => (279.4, 431.8),         // 11x17 inch
        PaperSize::C => (431.8, 558.8),         // 17x22 inch
        PaperSize::D => (558.8, 863.6),         // 22x34 inch
        PaperSize::E => (863.6, 1117.6),        // 34x44 inch
        PaperSize::USLetter => (215.9, 279.4),  // 8.5x11 inch
        PaperSize::USLegal => (215.9, 355.6),   // 8.5x14 inch
        PaperSize::USLedger => (279.4, 431.8),  // 11x17 inch
        PaperSize::Custom { width, height } => (*width, *height),
    }
}

/// Calculate the bounding box for a symbol instance
fn calculate_bounds(symbol: &SymbolInstance, lib_symbol: Option<&Symbol>, schematic: &Schematic) -> BoundingBox {
    let mut bounds = BoundingBox::new();

    // Get symbol transformation
    let sym_angle_rad = symbol.position.angle.to_radians();
    let cos_a = sym_angle_rad.cos();
    let sin_a = sym_angle_rad.sin();

    // Helper to transform a local point to world coordinates
    let transform_point = |local_x: f64, local_y: f64| -> (f64, f64) {
        // Apply mirror transformation if set
        let (mirrored_x, mirrored_y) = match symbol.mirror {
            Some(Mirror::X) => (local_x, -local_y),
            Some(Mirror::Y) => (-local_x, local_y),
            None => (local_x, local_y),
        };

        // Apply rotation
        let rotated_x = mirrored_x * cos_a - mirrored_y * sin_a;
        let rotated_y = mirrored_x * sin_a + mirrored_y * cos_a;

        // Translate by symbol position
        (rotated_x + symbol.position.x, rotated_y + symbol.position.y)
    };

    if let Some(lib_sym) = lib_symbol {
        // Include all graphics in the bounding box
        for unit in &lib_sym.units {
            for graphic in &unit.graphics {
                match graphic {
                    GraphicItem::Rectangle(rect) => {
                        let (x1, y1) = transform_point(rect.start.x, rect.start.y);
                        let (x2, y2) = transform_point(rect.end.x, rect.end.y);
                        bounds.include_point(x1, y1);
                        bounds.include_point(x2, y2);
                    }
                    GraphicItem::Polyline(poly) => {
                        for pt in &poly.points {
                            let (x, y) = transform_point(pt.x, pt.y);
                            bounds.include_point(x, y);
                        }
                    }
                    GraphicItem::Circle(circle) => {
                        let (cx, cy) = transform_point(circle.center.x, circle.center.y);
                        bounds.include_point(cx - circle.radius, cy - circle.radius);
                        bounds.include_point(cx + circle.radius, cy + circle.radius);
                    }
                    GraphicItem::Arc(arc) => {
                        let (x1, y1) = transform_point(arc.start.x, arc.start.y);
                        let (x2, y2) = transform_point(arc.mid.x, arc.mid.y);
                        let (x3, y3) = transform_point(arc.end.x, arc.end.y);
                        bounds.include_point(x1, y1);
                        bounds.include_point(x2, y2);
                        bounds.include_point(x3, y3);
                    }
                    GraphicItem::Text(_) => {
                        // Skip text for bounding box - it's typically labels
                    }
                }
            }

            // Include pin positions
            for pin in &unit.pins {
                let pin_pos = pin.position;
                let pin_length = pin.length;
                let pin_angle_rad = pin_pos.angle.to_radians();

                // Pin base (at symbol body)
                let (base_x, base_y) = transform_point(pin_pos.x, pin_pos.y);
                bounds.include_point(base_x, base_y);

                // Pin tip (wire connection point)
                let tip_local_x = pin_pos.x - pin_length * pin_angle_rad.cos();
                let tip_local_y = pin_pos.y + pin_length * pin_angle_rad.sin();
                let (tip_x, tip_y) = transform_point(tip_local_x, tip_local_y);
                bounds.include_point(tip_x, tip_y);
            }
        }
    }

    // Fallback: use pin positions from schematic if no lib_symbol or no graphics
    if !bounds.is_valid() {
        for (pos, _) in schematic.get_all_pin_positions(symbol) {
            bounds.include_point(pos.x, pos.y);
        }
    }

    // Final fallback: use symbol center point
    if !bounds.is_valid() {
        bounds.include_point(symbol.position.x, symbol.position.y);
    }

    bounds
}
