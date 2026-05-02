//! Semantic outline tool - Wire/connectivity-centric schematic view
//!
//! Provides a high-level view of schematic connectivity with automatic detection
//! of common passive component patterns (pullups, pulldowns, decoupling caps).

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::connectivity::endpoint::is_power_net_name;
use crate::schematic::Schematic;

/// Component type classification based on reference prefix or lib_id
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentType {
    Resistor,
    Capacitor { polarized: bool },
    Inductor,
    Diode,
    Transistor,
    IC,
    Connector,
    Relay,
    Switch,
    Other,
}

impl ComponentType {
    /// Classify a component by its lib_id
    /// Uses specific KiCAD Device library symbols for accurate classification
    pub fn classify(_reference: &str, lib_id: &str) -> Self {
        // Check for specific Device library symbols
        // These are the standard KiCAD symbols for passives
        match lib_id {
            // Resistors
            "Device:R" | "Device:R_Small" | "Device:R_US" | "Device:R_Small_US" => {
                ComponentType::Resistor
            }
            // Non-polarized capacitors
            "Device:C" | "Device:C_Small" => ComponentType::Capacitor { polarized: false },
            // Polarized capacitors
            "Device:C_Polarized" | "Device:C_Polarized_Small" | "Device:C_Polarized_US" => {
                ComponentType::Capacitor { polarized: true }
            }
            // Inductors
            "Device:L" | "Device:L_Small" | "Device:L_Core_Ferrite" | "Device:L_Core_Iron" => {
                ComponentType::Inductor
            }
            _ => {
                // For other components, use lib_id prefix matching
                if lib_id.starts_with("Diode:") || lib_id.starts_with("Device:LED") {
                    ComponentType::Diode
                } else if lib_id.starts_with("Transistor_") {
                    ComponentType::Transistor
                } else if lib_id.starts_with("Connector:") {
                    ComponentType::Connector
                } else if lib_id.starts_with("Relay:") {
                    ComponentType::Relay
                } else if lib_id.starts_with("Switch:") {
                    ComponentType::Switch
                } else {
                    ComponentType::Other
                }
            }
        }
    }

    /// Check if this is a 2-pin passive component that can be absorbed into semantic patterns
    pub fn is_two_pin_passive(&self) -> bool {
        matches!(
            self,
            ComponentType::Resistor | ComponentType::Capacitor { .. } | ComponentType::Inductor
        )
    }

    /// Check if this is a 2-pin component that can be merged into nets
    pub fn is_two_pin_mergeable(&self) -> bool {
        matches!(
            self,
            ComponentType::Resistor
                | ComponentType::Capacitor { .. }
                | ComponentType::Inductor
                | ComponentType::Switch
        )
    }

    /// Check if this is a polarized capacitor
    pub fn is_polarized_cap(&self) -> bool {
        matches!(self, ComponentType::Capacitor { polarized: true })
    }
}

/// Net type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetType {
    Power,
    Ground,
    Signal,
}

/// Net label scope - determines the visibility/reach of a net
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetScope {
    /// Global scope - visible across all sheets (from global_label or power symbols)
    #[default]
    Global,
    /// Hierarchical scope - connects parent/child sheets (from hierarchical_label)
    Hierarchical,
    /// Local scope - only visible within current sheet (from label)
    Local,
}

impl NetType {
    /// Classify a net by its name
    pub fn classify(name: &str) -> Self {
        let upper = name.to_uppercase();

        // Ground nets - explicitly GND variants only
        // Note: VSS is a power rail (negative supply), not ground
        if matches!(upper.as_str(), "GND" | "AGND" | "DGND" | "PGND" | "GNDA" | "GNDD") {
            return NetType::Ground;
        }

        // Power nets (includes VCC, VDD, VSS, +3V3, etc.)
        if is_power_net_name(name) {
            return NetType::Power;
        }

        NetType::Signal
    }

    pub fn is_ground(&self) -> bool {
        matches!(self, NetType::Ground)
    }

    pub fn is_power(&self) -> bool {
        matches!(self, NetType::Power)
    }
}

/// What a merged passive component connects to on its "other" side
#[derive(Debug, Clone, PartialEq)]
pub enum MergedTarget {
    /// Connected to a net (power, ground, or signal) with scope
    Net { name: String, scope: NetScope },
    /// Connected to a single component pin
    Pin { reference: String, pin: String },
    /// Connected to another merged component (for series chains)
    Merged(Box<SemanticConnection>),
}

impl MergedTarget {
    fn is_ground(&self) -> bool {
        matches!(self, MergedTarget::Net { name, .. } if NetType::classify(name).is_ground())
    }

    fn is_power(&self) -> bool {
        matches!(self, MergedTarget::Net { name, .. } if NetType::classify(name).is_power())
    }
}

impl fmt::Display for MergedTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergedTarget::Net { name, scope } => {
                let prefix = match scope {
                    NetScope::Global => '&',
                    NetScope::Hierarchical => '^',
                    NetScope::Local => '!',
                };
                write!(f, "{}{}", prefix, name)
            }
            MergedTarget::Pin { reference, pin } => write!(f, "{}:{}", reference, pin),
            MergedTarget::Merged(inner) => write!(f, "{}", inner),
        }
    }
}

/// A semantic connection endpoint
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticConnection {
    /// Regular pin reference (e.g., "U1:GPIO0")
    Pin { reference: String, pin: String },
    /// Net label with scope (e.g., "&VCC" for global, "^GPIO0" for hierarchical, "!SIGNAL" for local)
    Net { name: String, scope: NetScope },
    /// Merged resistor - specialized in Display based on target
    MergedResistor {
        reference: String,
        value: String,
        target: MergedTarget,
    },
    /// Merged capacitor - specialized in Display based on target
    MergedCapacitor {
        reference: String,
        value: String,
        polarized: bool,
        target: MergedTarget,
    },
    /// Merged switch
    MergedSwitch {
        reference: String,
        target: MergedTarget,
    },
}

impl fmt::Display for SemanticConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticConnection::Pin { reference, pin } => write!(f, "{}:{}", reference, pin),
            SemanticConnection::Net { name, scope } => {
                let prefix = match scope {
                    NetScope::Global => '&',
                    NetScope::Hierarchical => '^',
                    NetScope::Local => '!',
                };
                write!(f, "{}{}", prefix, name)
            }
            SemanticConnection::MergedResistor { reference, value, target } => {
                if target.is_power() {
                    write!(f, "$PULLUP({}, {}, {})", reference, value, target)
                } else if target.is_ground() {
                    write!(f, "$PULLDOWN({}, {})", reference, value)
                } else {
                    write!(f, "$RESISTOR({}, {}, {})", reference, value, target)
                }
            }
            SemanticConnection::MergedCapacitor { reference, value, polarized, target } => {
                if *polarized {
                    write!(f, "$CAPACITOR_POL({}, {}, {})", reference, value, target)
                } else {
                    write!(f, "$CAPACITOR({}, {}, {})", reference, value, target)
                }
            }
            SemanticConnection::MergedSwitch { reference, target } => {
                write!(f, "$SWITCH({}, {})", reference, target)
            }
        }
    }
}

/// A processed connection line with semantic annotations
#[derive(Debug, Clone)]
pub struct ProcessedNet {
    /// All endpoints in this net
    pub endpoints: Vec<SemanticConnection>,
}

impl fmt::Display for ProcessedNet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.endpoints.iter().map(|e| e.to_string()).collect();
        write!(f, "{}", parts.join(", "))
    }
}

/// A component in the semantic outline
#[derive(Debug, Clone)]
pub struct SemanticComponent {
    /// Reference designator (e.g., "U1")
    pub reference: String,
    /// Library ID (e.g., "Device:R")
    pub lib_id: String,
    /// Component value (e.g., "10k")
    pub value: String,
    /// Pin names/numbers
    pub pins: Vec<String>,
    /// Description/label if present
    pub description: Option<String>,
    /// Component type classification
    pub component_type: ComponentType,
}

/// A sheet (sub-schematic) in the semantic outline
#[derive(Debug, Clone)]
pub struct SemanticSheet {
    /// Sheet name (e.g., "Power", "RP2040")
    pub name: String,
    /// Sheet file path (e.g., "power.kicad_sch")
    pub file: String,
    /// Pin names on this sheet
    pub pins: Vec<String>,
}

/// The complete semantic outline of a schematic
#[derive(Debug, Clone)]
pub struct SemanticOutline {
    /// Components (excluding absorbed passives)
    pub components: Vec<SemanticComponent>,
    /// Sheets (sub-schematics) referenced by this schematic
    pub sheets: Vec<SemanticSheet>,
    /// Connections with semantic annotations
    pub connections: Vec<ProcessedNet>,
    /// References of components that were absorbed into semantic patterns
    pub absorbed_components: HashSet<String>,
}

impl SemanticOutline {
    /// Render the outline as text
    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push("Components:".to_string());

        // Group components by lib_id
        let mut grouped: HashMap<String, Vec<&SemanticComponent>> = HashMap::new();
        for comp in &self.components {
            grouped.entry(comp.lib_id.clone()).or_default().push(comp);
        }

        // Sort groups by lib_id
        let mut lib_ids: Vec<&String> = grouped.keys().collect();
        lib_ids.sort();

        for lib_id in lib_ids {
            let comps = &grouped[lib_id];

            // Collect and sort instances with their values
            let mut instances: Vec<(&str, &str)> = comps
                .iter()
                .map(|c| (c.reference.as_str(), c.value.as_str()))
                .collect();
            instances.sort_by(|a, b| {
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
                parse_ref(a.0).cmp(&parse_ref(b.0))
            });

            // Get the symbol name from lib_id (part after the colon)
            let symbol_name = lib_id.split(':').nth(1).unwrap_or("");

            // Format instances as "REF=VALUE" or just "REF" if value is redundant
            let instance_strs: Vec<String> = instances
                .iter()
                .map(|(ref_, val)| {
                    // Omit value if empty, equals reference, or equals the symbol name
                    if val.is_empty() || *val == *ref_ || *val == symbol_name {
                        ref_.to_string()
                    } else {
                        format!("{}={}", ref_, val)
                    }
                })
                .collect();

            // Use first component for shared properties (pins, description)
            let first = comps[0];

            lines.push(format!("  {}", lib_id));
            lines.push(format!("    Instances: {}", instance_strs.join(" ")));
            if !first.pins.is_empty() {
                lines.push(format!("    Pins: {}", first.pins.join(" ")));
            }
            if let Some(desc) = &first.description {
                lines.push(format!("    Label: {}", desc));
            }
        }

        // Sheets section (if any)
        if !self.sheets.is_empty() {
            lines.push(String::new());
            lines.push("Sheets:".to_string());

            for sheet in &self.sheets {
                lines.push(format!("  {}", sheet.name));
                lines.push(format!("    File: {}", sheet.file));
                if !sheet.pins.is_empty() {
                    lines.push(format!("    Pins: {}", sheet.pins.join(" ")));
                }
            }
        }

        lines.push(String::new());
        lines.push("Connections:".to_string());
        lines.push(String::new());

        for net in &self.connections {
            lines.push(net.to_string());
        }

        lines.join("\n")
    }
}

/// Internal structure for tracking component info during analysis
#[derive(Debug, Clone)]
struct ComponentData {
    reference: String,
    lib_id: String,
    value: String,
    pins: Vec<String>,
    pin_num_to_name: HashMap<String, String>,
    description: Option<String>,
    component_type: ComponentType,
}

/// Internal structure for tracking net info during analysis
#[derive(Debug, Clone)]
struct NetData {
    /// Net name (if any)
    name: Option<String>,
    /// Net scope
    scope: NetScope,
    /// Pin connections as (reference, pin)
    pins: Vec<(String, String)>,
}

/// Build semantic outline from a schematic
pub fn build_semantic_outline(schematic: &Schematic) -> SemanticOutline {
    // Step 1: Collect all components with their metadata
    let mut components: HashMap<String, ComponentData> = HashMap::new();

    for symbol in &schematic.symbols {
        let reference = symbol
            .properties
            .iter()
            .find(|p| p.name == "Reference")
            .map(|p| p.value.clone())
            .unwrap_or_else(|| "?".to_string());

        // Skip power symbols
        if reference.starts_with('#') {
            continue;
        }

        let value = symbol
            .properties
            .iter()
            .find(|p| p.name == "Value")
            .map(|p| p.value.clone())
            .unwrap_or_default();

        let description = symbol
            .properties
            .iter()
            .find(|p| p.name == "Description")
            .map(|p| p.value.clone())
            .filter(|v| !v.is_empty());

        let component_type = ComponentType::classify(&reference, &symbol.lib_id);

        // Get pin names from lib_symbol and build number→name mapping
        let lib_symbol = schematic.lib_symbols.iter().find(|s| s.name == symbol.lib_id);
        let (pins, pin_num_to_name): (Vec<String>, HashMap<String, String>) = if let Some(lib_sym) = lib_symbol {
            let mut pin_names: Vec<String> = Vec::new();
            let mut num_to_name: HashMap<String, String> = HashMap::new();

            for unit in &lib_sym.units {
                for p in &unit.pins {
                    // Prefer pin name over number if name is meaningful
                    let name = if p.name.name != "~" && !p.name.name.is_empty() {
                        p.name.name.clone()
                    } else {
                        p.number.number.clone()
                    };
                    pin_names.push(name.clone());
                    num_to_name.insert(p.number.number.clone(), name);
                }
            }

            pin_names.sort();
            pin_names.dedup();
            (pin_names, num_to_name)
        } else {
            let pins: Vec<String> = symbol.pins.iter().map(|p| p.number.clone()).collect();
            let num_to_name: HashMap<String, String> = symbol.pins.iter()
                .map(|p| (p.number.clone(), p.number.clone()))
                .collect();
            (pins, num_to_name)
        };

        components.insert(reference.clone(), ComponentData {
            reference,
            lib_id: symbol.lib_id.clone(),
            value,
            pins,
            pin_num_to_name,
            description,
            component_type,
        });
    }

    // Step 2: Build connectivity using the existing outline logic
    let raw_outline = super::outline::build_outline(schematic);

    // Helper to convert pin number to pin name
    let pin_name = |reference: &str, pin_num: &str| -> String {
        components
            .get(reference)
            .and_then(|c| c.pin_num_to_name.get(pin_num))
            .cloned()
            .unwrap_or_else(|| pin_num.to_string())
    };

    // Step 3: Build net data from raw outline (converting pin numbers to names)
    let mut net_data: Vec<NetData> = Vec::new();
    for net in &raw_outline.nets {
        let pins: Vec<(String, String)> = net
            .connections
            .iter()
            .filter_map(|conn| {
                let parts: Vec<&str> = conn.split(':').collect();
                if parts.len() == 2 {
                    let reference = parts[0];
                    let pin_num = parts[1];
                    Some((reference.to_string(), pin_name(reference, pin_num)))
                } else {
                    None
                }
            })
            .collect();

        // Convert NetLabelScope to NetScope
        let scope = match net.scope {
            super::outline::NetLabelScope::Global => NetScope::Global,
            super::outline::NetLabelScope::Hierarchical => NetScope::Hierarchical,
            super::outline::NetLabelScope::Local => NetScope::Local,
        };

        net_data.push(NetData {
            name: Some(net.name.clone()),
            scope,
            pins,
        });
    }

    // Step 4: Multi-pass merging of 2-pin components
    //
    // Pass 1: Collect all potential merge candidates with their raw targets
    // Pass 2: Loop until no changes - resolve nested targets (e.g., R9 -> SW2 -> GND becomes R9 -> $SWITCH(SW2, GND))
    // Pass 3: Build final ProcessedNet structures

    // Track which components are absorbed (merged into annotations)
    let mut absorbed: HashSet<String> = HashSet::new();

    // Merge candidate: (SemanticConnection with raw target, net_index where it appears)
    #[derive(Debug, Clone)]
    struct MergeCandidate {
        connection: SemanticConnection,
        net_idx: usize,
    }

    let mut merge_candidates: HashMap<String, MergeCandidate> = HashMap::new();

    // Pass 1: Identify all merge candidates
    for (net_idx, net) in net_data.iter().enumerate() {
        let net_type = net.name.as_ref().map(|n| NetType::classify(n));

        for (ref_, _pin) in &net.pins {
            let comp = match components.get(ref_.as_str()) {
                Some(c) if c.component_type.is_two_pin_mergeable() => c,
                _ => continue,
            };

            // Skip if already processed
            if merge_candidates.contains_key(ref_.as_str()) {
                continue;
            }

            // Get info about the other net this component connects to
            let other_info = match find_other_pin_net_info(&net_data, ref_, net.name.as_deref()) {
                Some(info) => info,
                None => continue,
            };

            let other_type = NetType::classify(&other_info.net_name);
            let this_is_power_ground = net_type.map(|t| t.is_power() || t.is_ground()).unwrap_or(false);
            let other_is_power_ground = other_type.is_power() || other_type.is_ground();
            let this_is_ground = net_type.map(|t| t.is_ground()).unwrap_or(false);
            let other_is_ground = other_type.is_ground();

            // Determine if we should merge this component into THIS net
            // Priority: always merge AWAY from power/ground nets
            let should_merge_here = if this_is_power_ground && !other_is_power_ground {
                false // THIS net is power/ground, other is not - don't merge here
            } else if other_is_power_ground && !this_is_power_ground {
                true // Other net is power/ground - merge into THIS net
            } else if this_is_ground && other_is_power_ground && !other_is_ground {
                false // Both power/ground, but THIS is GND and other is power - merge there
            } else if other_is_ground && this_is_power_ground && !this_is_ground {
                true // Both power/ground, but other is GND - merge here
            } else if other_info.single_other_pin.is_some() {
                true // Other net is "small" (component + 1 other pin) - merge here
            } else {
                false
            };

            if !should_merge_here {
                continue;
            }

            // Determine the raw target
            let target = if other_is_power_ground {
                MergedTarget::Net { name: other_info.net_name.clone(), scope: other_info.scope }
            } else if let Some(ref target_str) = other_info.single_other_pin {
                parse_merged_target(target_str, &other_info.net_name, other_info.scope)
            } else {
                continue;
            };

            // Create the merge candidate
            let connection = match comp.component_type {
                ComponentType::Resistor => SemanticConnection::MergedResistor {
                    reference: ref_.to_string(),
                    value: comp.value.clone(),
                    target,
                },
                ComponentType::Capacitor { polarized } => SemanticConnection::MergedCapacitor {
                    reference: ref_.to_string(),
                    value: comp.value.clone(),
                    polarized,
                    target,
                },
                ComponentType::Switch => SemanticConnection::MergedSwitch {
                    reference: ref_.to_string(),
                    target,
                },
                _ => continue,
            };

            merge_candidates.insert(ref_.to_string(), MergeCandidate {
                connection,
                net_idx,
            });
            absorbed.insert(ref_.to_string());
        }
    }

    // Pass 2: Resolve nested targets (loop until no changes)
    // If a candidate's target is a Pin of another candidate, nest it
    loop {
        let mut changed = false;

        // Collect updates to apply (can't mutate while iterating)
        let mut updates: Vec<(String, SemanticConnection)> = Vec::new();

        for (ref_, candidate) in &merge_candidates {
            let target_ref = match &candidate.connection {
                SemanticConnection::MergedResistor { target: MergedTarget::Pin { reference, .. }, .. } => reference,
                SemanticConnection::MergedCapacitor { target: MergedTarget::Pin { reference, .. }, .. } => reference,
                SemanticConnection::MergedSwitch { target: MergedTarget::Pin { reference, .. }, .. } => reference,
                _ => continue,
            };

            // Check if target is another merge candidate
            if let Some(nested_candidate) = merge_candidates.get(target_ref) {
                // Don't nest if the nested candidate also points to us (cycle)
                let nested_points_to_us = match &nested_candidate.connection {
                    SemanticConnection::MergedResistor { target: MergedTarget::Pin { reference, .. }, .. } => reference == ref_,
                    SemanticConnection::MergedCapacitor { target: MergedTarget::Pin { reference, .. }, .. } => reference == ref_,
                    SemanticConnection::MergedSwitch { target: MergedTarget::Pin { reference, .. }, .. } => reference == ref_,
                    _ => false,
                };

                if nested_points_to_us {
                    continue;
                }

                // Create nested version
                let nested_target = MergedTarget::Merged(Box::new(nested_candidate.connection.clone()));

                let new_connection = match &candidate.connection {
                    SemanticConnection::MergedResistor { reference, value, .. } => {
                        SemanticConnection::MergedResistor {
                            reference: reference.clone(),
                            value: value.clone(),
                            target: nested_target,
                        }
                    }
                    SemanticConnection::MergedCapacitor { reference, value, polarized, .. } => {
                        SemanticConnection::MergedCapacitor {
                            reference: reference.clone(),
                            value: value.clone(),
                            polarized: *polarized,
                            target: nested_target,
                        }
                    }
                    SemanticConnection::MergedSwitch { reference, .. } => {
                        SemanticConnection::MergedSwitch {
                            reference: reference.clone(),
                            target: nested_target,
                        }
                    }
                    _ => continue,
                };

                updates.push((ref_.clone(), new_connection));
                changed = true;
            }
        }

        // Apply updates
        for (ref_, new_connection) in updates {
            if let Some(candidate) = merge_candidates.get_mut(&ref_) {
                candidate.connection = new_connection;
            }
        }

        if !changed {
            break;
        }
    }

    // Find which candidates are nested inside others (should not appear at top level)
    let nested_refs: HashSet<String> = merge_candidates
        .values()
        .filter_map(|c| {
            match &c.connection {
                SemanticConnection::MergedResistor { target: MergedTarget::Merged(inner), .. } |
                SemanticConnection::MergedCapacitor { target: MergedTarget::Merged(inner), .. } |
                SemanticConnection::MergedSwitch { target: MergedTarget::Merged(inner), .. } => {
                    // Extract the reference from the nested connection
                    match inner.as_ref() {
                        SemanticConnection::MergedResistor { reference, .. } |
                        SemanticConnection::MergedCapacitor { reference, .. } |
                        SemanticConnection::MergedSwitch { reference, .. } => Some(reference.clone()),
                        _ => None,
                    }
                }
                _ => None,
            }
        })
        .collect();

    // Pass 3: Build final ProcessedNet structures
    let mut semantic_nets: Vec<ProcessedNet> = Vec::new();

    for (net_idx, net) in net_data.iter().enumerate() {
        let mut endpoints: Vec<SemanticConnection> = Vec::new();

        // Add merged components that belong to this net (and aren't nested inside others)
        for (ref_, candidate) in &merge_candidates {
            if candidate.net_idx == net_idx && !nested_refs.contains(ref_) {
                endpoints.push(candidate.connection.clone());
            }
        }

        // Add non-absorbed component pins
        for (ref_, pin) in &net.pins {
            if !absorbed.contains(ref_.as_str()) {
                endpoints.push(SemanticConnection::Pin {
                    reference: ref_.to_string(),
                    pin: pin.to_string(),
                });
            }
        }

        // Add net label (if not auto-generated)
        if let Some(name) = &net.name {
            if !name.starts_with("NET_") {
                endpoints.push(SemanticConnection::Net { name: name.clone(), scope: net.scope });
            }
        }

        // Only add nets with at least 2 endpoints
        if endpoints.len() >= 2 {
            semantic_nets.push(ProcessedNet { endpoints });
        }
    }

    // Step 5: Build final component list (excluding absorbed)
    let mut final_components: Vec<SemanticComponent> = components
        .into_iter()
        .filter(|(ref_, _)| !absorbed.contains(ref_))
        .map(|(_, data)| SemanticComponent {
            reference: data.reference,
            lib_id: data.lib_id,
            value: data.value,
            pins: data.pins,
            description: data.description,
            component_type: data.component_type,
        })
        .collect();

    // Sort by reference
    final_components.sort_by(|a, b| {
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

    // Sort nets by name for consistent output
    semantic_nets.sort_by(|a, b| {
        let get_sort_key = |net: &ProcessedNet| -> String {
            for ep in &net.endpoints {
                if let SemanticConnection::Net { name, .. } = ep {
                    return name.clone();
                }
            }
            // No net name - use first pin reference
            for ep in &net.endpoints {
                if let SemanticConnection::Pin { reference, .. } = ep {
                    return reference.clone();
                }
            }
            String::new()
        };
        get_sort_key(a).cmp(&get_sort_key(b))
    });

    // Step 6: Build sheets list
    let mut sheets: Vec<SemanticSheet> = schematic
        .sheets
        .iter()
        .map(|sheet| {
            let pins: Vec<String> = sheet.pins.iter().map(|p| p.name.clone()).collect();
            SemanticSheet {
                name: sheet.sheet_name.clone(),
                file: sheet.sheet_file.clone(),
                pins,
            }
        })
        .collect();

    // Sort sheets by name
    sheets.sort_by(|a, b| a.name.cmp(&b.name));

    SemanticOutline {
        components: final_components,
        sheets,
        connections: semantic_nets,
        absorbed_components: absorbed,
    }
}

/// Information about a component's connection on another net
struct OtherNetInfo {
    /// Name of the other net
    net_name: String,
    /// Net scope
    scope: NetScope,
    /// If there's exactly one other pin (not this component), its "ref:pin" string
    single_other_pin: Option<String>,
}

/// Parse a merged target from a "ref:pin" string and net name/scope
/// If the target is a power symbol (starts with #), use the net name instead
fn parse_merged_target(pin_str: &str, net_name: &str, scope: NetScope) -> MergedTarget {
    let parts: Vec<&str> = pin_str.split(':').collect();
    if parts.len() == 2 {
        let reference = parts[0];
        // Power symbols start with # - use the net name instead
        if reference.starts_with('#') {
            MergedTarget::Net { name: net_name.to_string(), scope }
        } else {
            MergedTarget::Pin {
                reference: reference.to_string(),
                pin: parts[1].to_string(),
            }
        }
    } else {
        // Fallback to net name
        MergedTarget::Net { name: net_name.to_string(), scope }
    }
}

/// Find detailed info about the net that the other pin of a 2-pin component is connected to
fn find_other_pin_net_info(nets: &[NetData], reference: &str, current_net: Option<&str>) -> Option<OtherNetInfo> {
    for net in nets {
        // Skip the current net
        if net.name.as_deref() == current_net {
            continue;
        }

        // Check if this component has a pin in this net
        let has_component = net.pins.iter().any(|(ref_, _)| ref_ == reference);
        if has_component {
            let other_pins: Vec<_> = net.pins.iter()
                .filter(|(ref_, _)| ref_ != reference)
                .collect();

            let single_other_pin = if other_pins.len() == 1 {
                Some(format!("{}:{}", other_pins[0].0, other_pins[0].1))
            } else {
                None
            };

            return Some(OtherNetInfo {
                net_name: net.name.clone().unwrap_or_default(),
                scope: net.scope,
                single_other_pin,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_type_classify() {
        // Standard Device library passives
        assert_eq!(ComponentType::classify("R1", "Device:R"), ComponentType::Resistor);
        assert_eq!(ComponentType::classify("R2", "Device:R_Small"), ComponentType::Resistor);
        assert_eq!(
            ComponentType::classify("C1", "Device:C"),
            ComponentType::Capacitor { polarized: false }
        );
        assert_eq!(
            ComponentType::classify("C2", "Device:C_Polarized"),
            ComponentType::Capacitor { polarized: true }
        );
        assert_eq!(ComponentType::classify("L1", "Device:L"), ComponentType::Inductor);

        // Other component types
        assert_eq!(ComponentType::classify("D1", "Diode:1N4148"), ComponentType::Diode);
        assert_eq!(ComponentType::classify("D2", "Device:LED"), ComponentType::Diode);
        assert_eq!(ComponentType::classify("Q1", "Transistor_BJT:2N3904"), ComponentType::Transistor);
        assert_eq!(ComponentType::classify("J1", "Connector:Conn_01x04"), ComponentType::Connector);
        assert_eq!(ComponentType::classify("K1", "Relay:Relay_DPDT"), ComponentType::Relay);

        // Unknown types
        assert_eq!(ComponentType::classify("U1", "MCU:ESP32"), ComponentType::Other);
    }

    #[test]
    fn test_net_type_classify() {
        // Ground variants
        assert_eq!(NetType::classify("GND"), NetType::Ground);
        assert_eq!(NetType::classify("AGND"), NetType::Ground);
        assert_eq!(NetType::classify("DGND"), NetType::Ground);
        // VSS is a power rail (negative supply), not ground
        assert_eq!(NetType::classify("VSS"), NetType::Power);
        // Other power nets
        assert_eq!(NetType::classify("VCC"), NetType::Power);
        assert_eq!(NetType::classify("VDD"), NetType::Power);
        assert_eq!(NetType::classify("+3V3"), NetType::Power);
        // Signal nets
        assert_eq!(NetType::classify("SDA"), NetType::Signal);
    }

    #[test]
    fn test_semantic_connection_display() {
        let pin = SemanticConnection::Pin {
            reference: "U1".to_string(),
            pin: "GPIO0".to_string(),
        };
        assert_eq!(pin.to_string(), "U1:GPIO0");

        // Global net
        let net = SemanticConnection::Net { name: "VCC".to_string(), scope: NetScope::Global };
        assert_eq!(net.to_string(), "&VCC");

        // Hierarchical net
        let hier_net = SemanticConnection::Net { name: "GPIO0".to_string(), scope: NetScope::Hierarchical };
        assert_eq!(hier_net.to_string(), "^GPIO0");

        // Local net
        let local_net = SemanticConnection::Net { name: "SIGNAL".to_string(), scope: NetScope::Local };
        assert_eq!(local_net.to_string(), "!SIGNAL");

        // Pullup: resistor to power net
        let pullup = SemanticConnection::MergedResistor {
            reference: "R1".to_string(),
            value: "10k".to_string(),
            target: MergedTarget::Net { name: "VCC".to_string(), scope: NetScope::Global },
        };
        assert_eq!(pullup.to_string(), "$PULLUP(R1, 10k, &VCC)");

        // Pulldown: resistor to ground
        let pulldown = SemanticConnection::MergedResistor {
            reference: "R2".to_string(),
            value: "10k".to_string(),
            target: MergedTarget::Net { name: "GND".to_string(), scope: NetScope::Global },
        };
        assert_eq!(pulldown.to_string(), "$PULLDOWN(R2, 10k)");

        // Series resistor to a pin
        let series = SemanticConnection::MergedResistor {
            reference: "R3".to_string(),
            value: "100".to_string(),
            target: MergedTarget::Pin { reference: "U1".to_string(), pin: "TX".to_string() },
        };
        assert_eq!(series.to_string(), "$RESISTOR(R3, 100, U1:TX)");

        // Non-polarized capacitor to ground
        let cap_gnd = SemanticConnection::MergedCapacitor {
            reference: "C1".to_string(),
            value: "100nF".to_string(),
            polarized: false,
            target: MergedTarget::Net { name: "GND".to_string(), scope: NetScope::Global },
        };
        assert_eq!(cap_gnd.to_string(), "$CAPACITOR(C1, 100nF, &GND)");

        // Polarized capacitor to ground
        let cap_polar_gnd = SemanticConnection::MergedCapacitor {
            reference: "C2".to_string(),
            value: "10uF".to_string(),
            polarized: true,
            target: MergedTarget::Net { name: "GND".to_string(), scope: NetScope::Global },
        };
        assert_eq!(cap_polar_gnd.to_string(), "$CAPACITOR_POL(C2, 10uF, &GND)");

        // Non-polarized capacitor to power net
        let cap_power = SemanticConnection::MergedCapacitor {
            reference: "C3".to_string(),
            value: "1uF".to_string(),
            polarized: false,
            target: MergedTarget::Net { name: "VCC".to_string(), scope: NetScope::Global },
        };
        assert_eq!(cap_power.to_string(), "$CAPACITOR(C3, 1uF, &VCC)");

        // Switch to ground
        let switch_gnd = SemanticConnection::MergedSwitch {
            reference: "SW1".to_string(),
            target: MergedTarget::Net { name: "GND".to_string(), scope: NetScope::Global },
        };
        assert_eq!(switch_gnd.to_string(), "$SWITCH(SW1, &GND)");

        // Switch to a pin
        let switch_pin = SemanticConnection::MergedSwitch {
            reference: "SW2".to_string(),
            target: MergedTarget::Pin { reference: "U1".to_string(), pin: "RESET".to_string() },
        };
        assert_eq!(switch_pin.to_string(), "$SWITCH(SW2, U1:RESET)");

        // Nested merge: resistor -> switch -> ground
        // This represents a series chain: R9 connects to SW2, SW2 connects to GND
        let nested = SemanticConnection::MergedResistor {
            reference: "R9".to_string(),
            value: "1K".to_string(),
            target: MergedTarget::Merged(Box::new(SemanticConnection::MergedSwitch {
                reference: "SW2".to_string(),
                target: MergedTarget::Net { name: "GND".to_string(), scope: NetScope::Global },
            })),
        };
        assert_eq!(nested.to_string(), "$RESISTOR(R9, 1K, $SWITCH(SW2, &GND))");

        // Nested merge: capacitor -> resistor -> pin
        let nested_cap = SemanticConnection::MergedCapacitor {
            reference: "C1".to_string(),
            value: "100nF".to_string(),
            polarized: false,
            target: MergedTarget::Merged(Box::new(SemanticConnection::MergedResistor {
                reference: "R1".to_string(),
                value: "10k".to_string(),
                target: MergedTarget::Pin { reference: "U1".to_string(), pin: "VDD".to_string() },
            })),
        };
        assert_eq!(nested_cap.to_string(), "$CAPACITOR(C1, 100nF, $RESISTOR(R1, 10k, U1:VDD))");
    }
}
