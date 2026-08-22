//! YAML to KiCAD schematic compiler

use std::collections::{HashMap, HashSet};

use crate::commands::{snap_to_grid, Command, ConnectCommand};
use crate::common::{Point, Position};
use crate::config::KicadConfig;
use crate::layout::{paper_dimensions, ForceDirectedLayout, LayoutConfig, LayoutEdge, LayoutGraph, LayoutNode, PinInfo};
use crate::schematic::{Mirror, PaperSize, Schematic, TitleBlock};
use crate::symbol::lookup::find_symbol;
use crate::symbol::Symbol;

use super::error::YamlError;
use super::types::{ComponentDef, SheetDef, YamlSchematic};
use super::validation::{validate_deep, ValidationResult};

/// Output from compiling a YAML schematic
#[derive(Debug, Clone)]
pub struct CompileOutput {
    /// Primary/root schematic for sheetless content and sheet definitions.
    pub root: Schematic,
    /// Child schematics keyed by sheet name.
    pub children: HashMap<String, Schematic>,
    /// Backwards-compatible alias for the root schematic.
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

    /// Compile a YAML schematic definition into KiCAD schematics.
    ///
    /// The root schematic contains all root-level components and all sheet definitions.
    /// Each named sheet is compiled into a child schematic and keyed by sheet name.
    pub fn compile(&self, yaml_sch: &YamlSchematic) -> Result<CompileOutput, YamlError> {
        // First validate the YAML schematic with deep symbol/pin checking
        let validation = validate_deep(yaml_sch, &self.config);
        if !validation.is_valid() {
            return Err(validation.errors.into_iter().next().unwrap());
        }

        let mut root = Schematic::new();
        let mut children: HashMap<String, Schematic> = HashMap::new();
        let mut components_placed = HashMap::new();
        let mut connections_made = 0;
        let mut warnings = validation.warnings;
        // Declared sheet pins that a connection actually wired up. `place_sheet` puts every
        // declared pin onto the sheet symbol unconditionally (it runs before connections are
        // processed and doesn't know yet which will be used), but a pin only ends up with a
        // matching hierarchical label when some cross-sheet connection references it — a
        // same-sheet-only connection, or no connection at all, never touches this path. Left
        // in place, such a pin has nothing inside the child sheet pointing back at it, so KiCAD
        // reports hier_label_mismatch. Tracked here so unused ones can be dropped afterward.
        let mut used_sheet_pins: HashSet<(String, String)> = HashSet::new();

        // The uuid every hierarchical instance path (component `instances`, a sheet symbol's own
        // `instances`) chains from — deliberately *not* `root.uuid`. See
        // `Schematic::hierarchy_root_uuid`'s doc comment for why the two must stay distinct.
        let hierarchy_root_uuid = uuid::Uuid::new_v4().to_string();
        root.hierarchy_root_uuid = Some(hierarchy_root_uuid.clone());

        self.apply_meta(&mut root, &yaml_sch.meta)?;

        // Sorted: `yaml_sch.sheets` is a HashMap, whose iteration order is randomized per
        // process — using it directly would assign a different page number to each sheet on
        // every recompile even when the yaml is unchanged (same non-determinism class as the
        // force-directed layout / cross-sheet wiring hub bugs fixed earlier).
        let mut sorted_sheet_names: Vec<&String> = yaml_sch.sheets.keys().collect();
        sorted_sheet_names.sort();

        // Page 1 is reserved for the root; each child sheet gets the next one, in sorted-name
        // order, matching how real KiCad numbers sheets in a project.
        for (index, sheet_name) in sorted_sheet_names.into_iter().enumerate() {
            let sheet_def = &yaml_sch.sheets[sheet_name];
            self.place_sheet(&mut root, sheet_name, sheet_def)?;
            let mut child = Schematic::new();
            self.apply_meta(&mut child, &yaml_sch.meta)?;
            // Real KiCad child sheet files carry no `sheet_instances` block at all — only the
            // root file declares itself as page 1. The page-number/instances bookkeeping for
            // *this* sheet lives in the root's own `(sheet ...)` block instead (set below).
            child.sheet_instances = Vec::new();

            if let Some(sheet) = root.sheets.iter_mut().find(|s| s.sheet_name == *sheet_name) {
                let page = (index + 2).to_string();
                child.hierarchy_path_prefix =
                    Some(Self::sheet_instance_path(&hierarchy_root_uuid, &sheet.uuid));

                sheet.instances = vec![crate::schematic::SheetProjectInstance {
                    project_name: String::new(),
                    paths: vec![crate::schematic::SheetInstance {
                        path: format!("/{}", hierarchy_root_uuid),
                        page,
                    }],
                }];
            }

            children.insert(sheet_name.clone(), child);
        }

        let all_components = yaml_sch.all_components();
        let all_connections = yaml_sch.all_connections();

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

        let mut by_sheet: HashMap<String, Vec<(&String, &ComponentDef)>> = HashMap::new();
        for (reference, component) in &all_components {
            let sheet_name = component
                .sheet
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("__root__")
                .to_string();
            by_sheet.entry(sheet_name).or_default().push((reference, component));
        }

        // Layout runs once *per sheet*, not once globally across every component in the yaml.
        // A single global pass would place all components (99 in a real project) relative to
        // each other on one shared page, then split them into child sheets afterward with their
        // global-layout positions unchanged — so most components land far outside whichever
        // individual sheet's page they actually end up on. Each sheet's own components (plus any
        // left on the root) instead get their own fresh page and their own force-directed pass;
        // cross-sheet connections are harmless to leave in since the layout algorithm silently
        // skips edges whose other endpoint isn't in that pass's graph.
        let mut computed_positions: HashMap<String, Point> = HashMap::new();
        for entries in by_sheet.values() {
            if entries.iter().any(|(_, component)| component.position.is_none()) {
                let sheet_positions =
                    self.compute_layout(yaml_sch, entries, &all_connections, &symbols)?;
                computed_positions.extend(sheet_positions);
            }
        }

        for (sheet_name, entries) in &by_sheet {
            let target = if sheet_name == "__root__" {
                &mut root
            } else {
                children.get_mut(sheet_name).ok_or_else(|| {
                    YamlError::Other(format!("Sheet '{}' is defined but not compiled", sheet_name))
                })?
            };

            for (reference, component) in entries {
                let position = computed_positions.get(*reference).cloned();
                let lib_id = self.place_component(target, reference, component, position)?;
                components_placed.insert((*reference).clone(), lib_id);
            }
        }

        for connection in &all_connections {
            let mut sheet_names: HashSet<String> = HashSet::new();
            for pin_ref in &connection.pins {
                let parts: Vec<&str> = pin_ref.splitn(2, ':').collect();
                if parts.len() != 2 {
                    continue;
                }

                let reference = parts[0];
                let component = all_components.get(reference);
                let sheet_name = component
                    .and_then(|c| c.sheet.as_deref())
                    .filter(|name| !name.trim().is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| "__root__".to_string());
                sheet_names.insert(sheet_name);
            }

            if sheet_names.is_empty() {
                sheet_names.insert("__root__".to_string());
            }

            if sheet_names.len() == 1 {
                let sheet_name = sheet_names.iter().next().unwrap();
                if sheet_name == "__root__" {
                    self.create_connection(&mut root, connection)?;
                } else {
                    let target = children.get_mut(sheet_name).ok_or_else(|| {
                        YamlError::Other(format!(
                            "Connection {:?} references sheet '{}' but no child schematic was created",
                            connection, sheet_name
                        ))
                    })?;
                    self.create_connection(target, connection)?;
                }
                connections_made += 1;
                continue;
            }

            // Check if this is a global or power net that can bypass sheet pin requirements
            let is_global_net = connection.global.unwrap_or(false);
            let is_power_net = connection.net.as_ref()
                .map(|net| {
                    let upper = net.to_uppercase();
                    // Common ground names
                    matches!(upper.as_str(), "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GNDA" | "GNDD") ||
                    // VCC variants
                    upper.starts_with("VCC") || upper.starts_with("VDD") || upper.starts_with("VSS") ||
                    // Voltage rails like +3V3, +5V, +12V, -5V, etc.
                    ((upper.starts_with('+') || upper.starts_with('-')) && upper.contains('V')) ||
                    // Common power names
                    matches!(upper.as_str(), "3V3" | "5V" | "12V" | "1V8" | "2V5" | "VBAT" | "VIN" | "VOUT")
                })
                .unwrap_or(false);

            // Find pins that are on the root sheet
            let root_pins: Vec<(&str, &str)> = connection.pins
                .iter()
                .filter_map(|pin_ref| {
                    let parts: Vec<&str> = pin_ref.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let reference = parts[0];
                        let pin = parts[1];
                        if let Some(component) = all_components.get(reference) {
                            let component_sheet = component
                                .sheet
                                .as_deref()
                                .filter(|name| !name.trim().is_empty())
                                .unwrap_or("__root__");
                            if component_sheet == "__root__" {
                                return Some((reference, pin));
                            }
                        }
                    }
                    None
                })
                .collect();

            let mut handled = false;
            // Positions of declared sheet pins this connection must wire root-side pins to.
            // A hierarchical label only propagates a net onto the parent sheet symbol's matching
            // pin, so anything on the root that shares the net has to be physically wired to that
            // pin's exact position (not just labeled) or it stays electrically isolated.
            let mut hierarchical_targets: Vec<Point> = Vec::new();
            // Sorted: `sheet_names` is a HashSet, whose iteration order is randomized per
            // process. The first sheet visited here determines which point becomes the wiring
            // "hub" (root_side_points[0]) below, so an unsorted order means the exact same yaml
            // can compile to a different (though still electrically valid) wire topology on
            // different invocations — sorting makes which sheet is treated as the hub
            // deterministic given identical input.
            let mut sorted_sheet_names: Vec<&String> = sheet_names.iter().collect();
            sorted_sheet_names.sort();
            for sheet_name in sorted_sheet_names {
                if sheet_name == "__root__" {
                    continue;
                }

                let target = children.get_mut(sheet_name).ok_or_else(|| {
                    YamlError::Other(format!(
                        "Connection {:?} crosses sheet '{}' but no child schematic was created",
                        connection, sheet_name
                    ))
                })?;

                // Find all pins from this sheet that are part of this connection
                let pins_in_sheet: Vec<(&str, &str)> = connection.pins
                    .iter()
                    .filter_map(|pin_ref| {
                        let parts: Vec<&str> = pin_ref.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            let reference = parts[0];
                            let pin = parts[1];
                            if let Some(component) = all_components.get(reference) {
                                let component_sheet = component
                                    .sheet
                                    .as_deref()
                                    .filter(|name| !name.trim().is_empty())
                                    .unwrap_or("__root__");
                                if component_sheet == sheet_name {
                                    return Some((reference, pin));
                                }
                            }
                        }
                        None
                    })
                    .collect();

                if pins_in_sheet.is_empty() {
                    continue;
                }

                let sheet_def = yaml_sch.sheets.get(sheet_name);
                let pin_name = connection.net.clone().unwrap_or_else(|| {
                    sheet_def
                        .and_then(|def| def.pins.first())
                        .map(|pin| pin.name.clone())
                        .unwrap_or_else(|| "net".to_string())
                });

                // For global/power nets, use default hierarchical label shape
                // For explicit nets, require sheet pin definition
                let sheet_pin = sheet_def
                    .and_then(|def| def.pins.iter().find(|p| p.name == pin_name));

                if sheet_pin.is_none() && !is_global_net && !is_power_net {
                    // Non-global, non-power cross-sheet connections require explicit sheet pins
                    continue;
                }

                // A declared sheet pin means the user opted into a properly-scoped hierarchical
                // connection: label the pins inside the child AND wire root-side pins to the
                // pin's exact position on the parent sheet symbol. Without a declared pin (pure
                // global/power net), fall back to real global labels, which connect by name
                // anywhere in the project and need no matching sheet pin at all.
                if let Some(declared_pin) = sheet_pin {
                    let label_shape = crate::schematic::LabelShape::from_str(
                        declared_pin.shape.as_deref().unwrap_or("input"),
                    );
                    // Read back the already-placed (and grid-snapped) sheet pin from `root.sheets`
                    // rather than the raw yaml position, so this matches exactly what wire-endpoint
                    // snapping will target.
                    if let Some(placed_pin) = root
                        .sheets
                        .iter()
                        .find(|s| s.sheet_name == *sheet_name)
                        .and_then(|s| s.pins.iter().find(|p| p.name == declared_pin.name))
                    {
                        hierarchical_targets.push(Point::new(placed_pin.position.x, placed_pin.position.y));
                        used_sheet_pins.insert((sheet_name.clone(), declared_pin.name.clone()));
                    }

                    for (reference, pin) in pins_in_sheet {
                        if let Some(symbol) = target.find_symbol_by_reference(reference) {
                            if let Some((pos, pin_angle)) = target.get_pin_position(symbol, pin) {
                                let label_angle = match (pin_angle as i32) % 360 {
                                    0 => 0.0,     // Pin points right
                                    90 => 90.0,   // Pin points up
                                    180 => 180.0, // Pin points left
                                    270 => 270.0, // Pin points down
                                    a if a < 0 => ((a + 360) % 360) as f64,
                                    _ => pin_angle,
                                };
                                target.hierarchical_labels.push(crate::schematic::HierarchicalLabel {
                                    text: pin_name.clone(),
                                    shape: label_shape,
                                    position: crate::common::Position::new(pos.x, pos.y, label_angle),
                                    fields_autoplaced: false,
                                    effects: None,
                                    uuid: uuid::Uuid::new_v4().to_string(),
                                    properties: Vec::new(),
                                });
                                handled = true;
                            }
                        }
                    }
                } else {
                    let label_shape = if is_power_net {
                        crate::schematic::LabelShape::Passive
                    } else {
                        crate::schematic::LabelShape::Input
                    };

                    for (reference, pin) in pins_in_sheet {
                        if let Some(symbol) = target.find_symbol_by_reference(reference) {
                            if let Some((pos, pin_angle)) = target.get_pin_position(symbol, pin) {
                                let label_angle = match (pin_angle as i32) % 360 {
                                    0 => 0.0,
                                    90 => 90.0,
                                    180 => 180.0,
                                    270 => 270.0,
                                    a if a < 0 => ((a + 360) % 360) as f64,
                                    _ => pin_angle,
                                };
                                target.add_global_label(
                                    &pin_name,
                                    crate::common::Position::new(pos.x, pos.y, label_angle),
                                    label_shape,
                                );
                                handled = true;
                            }
                        }
                    }
                }
            }

            if hierarchical_targets.is_empty() {
                // No declared sheet pin was involved anywhere in this connection: fall back to
                // the global-label path for any root-side pins, same as the single-sheet case.
                // Only the root-side pins are passed in — the full `connection` also lists pins
                // that live on child sheets, and those references don't resolve inside the root
                // schematic.
                if !root_pins.is_empty() {
                    let root_connection = super::types::Connection {
                        net: connection.net.clone(),
                        pins: root_pins
                            .iter()
                            .map(|(reference, pin)| format!("{}:{}", reference, pin))
                            .collect(),
                        global: connection.global,
                    };
                    self.create_connection(&mut root, &root_connection)?;
                } else if !handled && !is_global_net && !is_power_net {
                    return Err(YamlError::Other(format!(
                        "Connection {:?} crosses sheet boundaries but no matching sheet pin definitions were found (use global: true or declare sheet pins)",
                        connection
                    )));
                }
            } else {
                // At least one declared sheet pin is part of this connection: every point that
                // shares the net on the root schematic — root component pins AND every involved
                // sheet symbol's pin (when two sibling sheets both declare a pin for this net)
                // — needs an actual wire between them. A hierarchical label only reaches as far
                // as the matching sheet pin; nothing propagates further on the root by name alone.
                let mut root_side_points: Vec<Point> = root_pins
                    .iter()
                    .filter_map(|(reference, pin)| {
                        let symbol = root.find_symbol_by_reference(reference)?;
                        root.get_pin_position(symbol, pin).map(|(pos, _)| pos)
                    })
                    .collect();
                root_side_points.extend(hierarchical_targets.iter().copied());

                if root_side_points.len() >= 2 {
                    let hub = root_side_points[0];
                    if root_side_points.len() > 2 {
                        root.add_junction(hub);
                    }
                    for pos in &root_side_points[1..] {
                        // Direct (not Orthogonal) routing is deliberate: when many sheet-crossing
                        // nets share a common "hub row" — e.g. many declared pins along one edge
                        // of a sheet, each getting its own hub-and-spoke wire here — Orthogonal's
                        // L-shaped route runs its horizontal leg along that shared edge for every
                        // one of them, so unrelated nets' wire segments overlap collinearly over a
                        // wide shared span. KiCAD's ERC then non-deterministically (by whichever
                        // sheet happens to end up as the hub, itself HashSet-order-dependent, not
                        // yaml order) misattributes connectivity for a fraction of them —
                        // label_dangling on some, clean on geometrically identical neighbors.
                        // Verified: switching to Direct across dense edge-packing repros (19 nets
                        // on one 700-unit edge; a net with 2-3 same-sheet pins mixed into a dense
                        // edge) eliminates it entirely, with no other regressions.
                        root.add_wire_routed(hub, *pos, crate::schematic::RoutingMode::Direct);
                    }
                }
            }

            connections_made += 1;
        }

        // Drop any declared sheet pin that no connection actually wired up (see the comment on
        // `used_sheet_pins` above) rather than shipping a schematic KiCAD will flag as broken.
        for sheet in &mut root.sheets {
            let sheet_name = sheet.sheet_name.clone();
            sheet.pins.retain(|pin| {
                let used = used_sheet_pins.contains(&(sheet_name.clone(), pin.name.clone()));
                if !used {
                    warnings.push(format!(
                        "Sheet '{}' declares pin '{}' but no connection wires it up (same-sheet \
                         connections don't reach it, and neither does an unused declaration) — omitted",
                        sheet_name, pin.name
                    ));
                }
                used
            });
        }

        let root_schematic = root.clone();
        Ok(CompileOutput {
            root: root_schematic.clone(),
            children: children.clone(),
            schematic: root_schematic,
            components_placed,
            connections_made,
            warnings,
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

            // If position is specified, use as starting position (not fixed, can be adjusted)
            if let Some(ref pos) = component.position {
                node = node.with_position(pos.x(), pos.y());
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

    /// Build the KiCad hierarchy path for a child sheet: `/{hierarchy_root_uuid}/{sheet_uuid}`.
    /// `hierarchy_root_uuid` must be the project-wide hierarchy-path root uuid (see
    /// `Schematic::hierarchy_root_uuid`), not any individual file's own uuid.
    fn sheet_instance_path(hierarchy_root_uuid: &str, sheet_uuid: &str) -> String {
        format!("/{}/{}", hierarchy_root_uuid, sheet_uuid)
    }

    /// Place a hierarchical sheet definition in the schematic
    fn place_sheet(
        &self,
        schematic: &mut Schematic,
        sheet_name: &str,
        sheet_def: &SheetDef,
    ) -> Result<(), YamlError> {
        let position = sheet_def
            .position
            .as_ref()
            .map(|pos| Position::new(pos.x(), pos.y(), 0.0))
            .unwrap_or_default();

        let size = sheet_def
            .size
            .map(|[w, h]| (w, h))
            .unwrap_or((200.0, 150.0));

        let path = sheet_def
            .path
            .clone()
            .unwrap_or_else(|| sheet_name.to_string());

        // KiCAD resolves the child sheet's screen from this property by filename on disk,
        // so it must carry the extension even though the yaml `path` typically omits it.
        let sheet_file_property = if path.ends_with(".kicad_sch") {
            path.clone()
        } else {
            format!("{}.kicad_sch", path)
        };

        let pins = sheet_def
            .pins
            .iter()
            .map(|pin| {
                let raw_x = pin.position.as_ref().map(|pos| pos.x()).unwrap_or(0.0);
                let raw_y = pin.position.as_ref().map(|pos| pos.y()).unwrap_or(0.0);

                // KiCAD orients a sheet pin's glyph to match whichever edge of the sheet
                // rectangle it sits on (verified against hand-authored .kicad_sch files):
                // right edge -> 0, left edge -> 180, top edge -> 90, bottom edge -> 270.
                // A wrong angle here (e.g. always 0) makes KiCAD treat the pin as
                // disconnected even when a wire lands exactly on its coordinates.
                // Edge detection uses the *raw* yaml coordinates against the sheet's raw
                // bounds — the sheet's width/height are rarely exact multiples of the grid
                // (e.g. a 200mm-wide sheet), so comparing already-snapped values against an
                // unsnapped edge would miss the match entirely.
                //
                // A pin that isn't on any edge at all (interior point, typo, or a sheet with
                // no explicit position/size so it defaults to (0,0)-(200,150)) can't be wired
                // correctly no matter what angle is guessed — it silently produces
                // wire_dangling/pin_not_connected in kicad-cli sch erc with no indication why.
                // Fail loudly here instead, at the point where the bad coordinate is known.
                const EPS: f64 = 0.01;
                let angle = if (raw_x - position.x).abs() < EPS {
                    180.0
                } else if (raw_x - (position.x + size.0)).abs() < EPS {
                    0.0
                } else if (raw_y - position.y).abs() < EPS {
                    90.0
                } else if (raw_y - (position.y + size.1)).abs() < EPS {
                    270.0
                } else {
                    return Err(YamlError::Other(format!(
                        "Sheet '{}' pin '{}' at ({}, {}) is not on the border of the sheet rectangle \
                         ({}, {}) to ({}, {}) — sheet pins must sit exactly on the left/right/top/bottom \
                         edge (within {} units) or KiCAD can't wire them",
                        sheet_name, pin.name, raw_x, raw_y,
                        position.x, position.y, position.x + size.0, position.y + size.1,
                        EPS
                    )));
                };

                // Snapped to match the grid every wire endpoint is snapped to (see
                // `create_connection`'s wiring of root-side pins to this exact position) —
                // otherwise a wire aimed at the raw yaml coordinate lands a fraction of a
                // grid cell short and ERC reports it as unconnected.
                let x = snap_to_grid(raw_x);
                let y = snap_to_grid(raw_y);

                Ok(crate::schematic::SheetPin {
                    name: pin.name.clone(),
                    shape: crate::schematic::LabelShape::from_str(
                        pin.shape.as_deref().unwrap_or("input"),
                    ),
                    position: Position::new(x, y, angle),
                    uuid: uuid::Uuid::new_v4().to_string(),
                })
            })
            .collect::<Result<Vec<_>, YamlError>>()?;

        schematic.sheets.push(crate::schematic::Sheet {
            position,
            size,
            uuid: uuid::Uuid::new_v4().to_string(),
            sheet_name: sheet_name.to_string(),
            sheet_file: sheet_file_property,
            pins,
            dnp: false,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            fields_autoplaced: false,
            // Populated later in compile()'s sheets loop, once this sheet's page number is
            // known.
            instances: Vec::new(),
        });

        Ok(())
    }

    /// Compute layout positions for one sheet's worth of components (the root's own unsheeted
    /// components count as a "sheet" here too) that don't have explicit positions. Called once
    /// per sheet — see the comment at the call site for why a single global pass across every
    /// component in the yaml doesn't work once components are split across sheets.
    fn compute_layout(
        &self,
        yaml_sch: &YamlSchematic,
        components: &[(&String, &ComponentDef)],
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
        for &(reference, component) in components {
            let symbol = symbols.get(reference);
            let (width, height) = Self::calculate_symbol_bounds(symbol);

            let mut node = LayoutNode::new(reference.clone(), (width, height));

            // Set group if component is in a group
            if let Some(group_name) = component_groups.get(reference) {
                node = node.with_group(Some(group_name.clone()));
            }

            // If position is specified, use as starting position (not fixed, can be adjusted)
            if let Some(ref pos) = component.position {
                node = node.with_position(pos.x(), pos.y());
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
    fn test_compile_sheet_definitions() {
        let yaml = r#"
components:
  U1:
    symbol: Device:R
    position: [100, 50]
    value: 10k
    sheet: Power

sheets:
  Power:
    path: power
    position: [0, 0]
    size: [200, 150]
"#;
        let result = compile_yaml_str(yaml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let output = result.unwrap();
        assert_eq!(output.root.sheets.len(), 1);
        assert_eq!(output.root.sheets[0].sheet_name, "Power");
        assert_eq!(output.root.sheets[0].sheet_file, "power.kicad_sch");
        assert_eq!(output.children.len(), 1);
        assert!(output.children.contains_key("Power"));

        // Real KiCad child sheet files carry no `sheet_instances` block at all — only the root
        // file declares itself as page 1 (verified against real GUI-saved multi-sheet project
        // files: only the root .kicad_sch had a top-level `sheet_instances`, none of 12 child
        // files did).
        let child = output.children.get("Power").unwrap();
        assert_eq!(child.sheet_instances.len(), 0);

        // The root's own sheet block carries the page-number bookkeeping instead: path is just
        // "/{hierarchy_root_uuid}" (no sheet uuid suffix — that's what real KiCad emits for a
        // sheet symbol's own instances entry, distinct from a *component's* instances path
        // inside the child, which does include the sheet uuid).
        let hierarchy_root_uuid = output
            .root
            .hierarchy_root_uuid
            .as_ref()
            .expect("compile() should always set hierarchy_root_uuid");
        assert_ne!(hierarchy_root_uuid, &output.root.uuid);

        let sheet = &output.root.sheets[0];
        assert_eq!(sheet.instances.len(), 1);
        assert_eq!(sheet.instances[0].paths.len(), 1);
        assert_eq!(
            sheet.instances[0].paths[0].path,
            format!("/{}", hierarchy_root_uuid)
        );
        assert_eq!(sheet.instances[0].paths[0].page, "2");

        // A component placed on the child sheet still needs the full chain (root + sheet uuid)
        // in its own instances path.
        let u1 = child
            .find_symbol_by_reference("U1")
            .expect("U1 should be placed");
        assert_eq!(
            u1.instances[0].paths[0].path,
            format!("/{}/{}", hierarchy_root_uuid, sheet.uuid)
        );
    }

    #[test]
    fn test_compile_same_sheet_connection_stays_on_child_schematic() {
        let yaml = r#"
components:
  U1:
    symbol: Device:R
    position: [100, 50]
    sheet: Power
  U2:
    symbol: Device:R
    position: [200, 50]
    sheet: Power

sheets:
  Power:
    path: power
    position: [0, 0]
    size: [200, 150]
    pins:
      - name: VCC
        shape: output
        position: [0, 50]

connections:
  - net: VCC
    pins: [U1:1, U2:1]
"#;
        let output = compile_yaml_str(yaml).unwrap();
        let power_child = output.children.get("Power").unwrap();

        assert!(!power_child.wires.is_empty() || !power_child.labels.is_empty() || !power_child.global_labels.is_empty());

        // The declared VCC pin sits on a valid edge, but only a same-sheet connection ever
        // references it — no cross-sheet connection wires it up, so it should be dropped rather
        // than left as a dangling sheet pin, with a warning explaining why.
        assert!(output.root.sheets[0].pins.is_empty());
        assert!(output.warnings.iter().any(|w| w.contains("VCC") && w.contains("Power")));
    }

    #[test]
    fn test_compile_cross_sheet_connection_uses_hierarchical_labels() {
        let yaml = r#"
components:
  U1:
    symbol: Device:R
    position: [100, 50]
    sheet: Power
  U2:
    symbol: Device:R
    position: [200, 50]
    sheet: Control

sheets:
  Power:
    path: power
    position: [0, 0]
    size: [200, 150]
    pins:
      - name: VCC
        shape: output
        position: [200, 50]
  Control:
    path: control
    position: [250, 0]
    size: [200, 150]
    pins:
      - name: VCC
        shape: input
        position: [250, 50]

connections:
  - net: VCC
    pins: [U1:1, U2:1]
"#;
        let output = compile_yaml_str(yaml).unwrap();

        let power_child = output.children.get("Power").unwrap();
        let control_child = output.children.get("Control").unwrap();

        assert!(!power_child.hierarchical_labels.is_empty());
        assert!(!control_child.hierarchical_labels.is_empty());
        assert!(power_child.hierarchical_labels.iter().any(|label| label.text == "VCC"));
        assert!(control_child.hierarchical_labels.iter().any(|label| label.text == "VCC"));
    }

    #[test]
    fn test_compile_power_net_cross_sheet_without_sheet_pins() {
        // Test that power nets (like GND, +3V3) automatically create hierarchical labels
        // without requiring explicit sheet pin definitions
        let yaml = r#"
components:
  U1:
    symbol: Device:R
    position: [100, 50]
    sheet: Power
  U2:
    symbol: Device:R
    position: [200, 50]
    sheet: Control

sheets:
  Power:
    path: power
  Control:
    path: control

connections:
  - net: GND
    pins: [U1:1, U2:1]
  - net: +3V3
    pins: [U1:2, U2:2]
"#;
        let output = compile_yaml_str(yaml).unwrap();

        let power_child = output.children.get("Power").unwrap();
        let control_child = output.children.get("Control").unwrap();

        // Without a declared sheet pin, a hierarchical label would have no matching pin on the
        // parent sheet symbol (hier_label_mismatch in KiCAD ERC). Power nets instead get real
        // global labels, which connect by name project-wide with no sheet pin required.
        assert!(!power_child.global_labels.is_empty());
        assert!(!control_child.global_labels.is_empty());
        assert!(power_child.global_labels.iter().any(|label| label.text == "GND"));
        assert!(control_child.global_labels.iter().any(|label| label.text == "GND"));
        assert!(power_child.global_labels.iter().any(|label| label.text == "+3V3"));
        assert!(control_child.global_labels.iter().any(|label| label.text == "+3V3"));
    }

    #[test]
    fn test_compile_global_flag_cross_sheet_without_sheet_pins() {
        // Test that connections marked global: true bypass sheet pin requirement
        let yaml = r#"
components:
  U1:
    symbol: Device:R
    position: [100, 50]
    sheet: Power
  U2:
    symbol: Device:R
    position: [200, 50]
    sheet: Control

sheets:
  Power:
    path: power
  Control:
    path: control

connections:
  - net: DATA_BUS
    global: true
    pins: [U1:1, U2:1]
"#;
        let output = compile_yaml_str(yaml).unwrap();

        let power_child = output.children.get("Power").unwrap();
        let control_child = output.children.get("Control").unwrap();

        // With global: true, even non-power nets bypass the sheet-pin requirement via real
        // global labels (same reasoning as the power-net case above).
        assert!(!power_child.global_labels.is_empty());
        assert!(!control_child.global_labels.is_empty());
        assert!(power_child.global_labels.iter().any(|label| label.text == "DATA_BUS"));
        assert!(control_child.global_labels.iter().any(|label| label.text == "DATA_BUS"));
    }

    #[test]
    fn test_compile_auto_layout_stays_within_each_sheets_own_page() {
        // Auto-layout used to run once globally across every component in the yaml, before
        // components were split into their target sheets — so a sheet's components landed
        // wherever the force-directed pass put them relative to *every other sheet's*
        // components too, frequently well outside that sheet's own page bounds once split out.
        // Each sheet must now get its own independent layout pass against its own page.
        let yaml = r#"
meta:
  paper: A4

components:
  UA0:
    symbol: Device:R
  UA1:
    symbol: Device:R
  UA2:
    symbol: Device:R
  UA3:
    symbol: Device:R
  UB0:
    symbol: Device:R
    sheet: SideB
  UB1:
    symbol: Device:R
    sheet: SideB
  UB2:
    symbol: Device:R
    sheet: SideB
  UB3:
    symbol: Device:R
    sheet: SideB

sheets:
  SideB:
    path: side_b_layout_bounds

connections:
  - pins: [UA0:2, UA1:1]
  - pins: [UA1:2, UA2:1]
  - pins: [UA2:2, UA3:1]
  - pins: [UB0:2, UB1:1]
  - pins: [UB1:2, UB2:1]
  - pins: [UB2:2, UB3:1]
"#;
        let output = compile_yaml_str(yaml).expect("compile_yaml_str failed");

        let (paper_w, paper_h) = crate::layout::paper_dimensions("A4");
        let assert_in_bounds = |label: &str, schematic: &Schematic| {
            for symbol in &schematic.symbols {
                let p = symbol.position;
                assert!(
                    p.x >= 0.0 && p.x <= paper_w && p.y >= 0.0 && p.y <= paper_h,
                    "{label}: symbol {} placed at ({}, {}), outside the {paper_w}x{paper_h} A4 page",
                    symbol.lib_id, p.x, p.y
                );
            }
        };

        assert_in_bounds("root (UA0-3)", &output.root);
        assert_in_bounds("SideB (UB0-3)", output.children.get("SideB").unwrap());
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
