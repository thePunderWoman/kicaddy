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
    /// Connected to a net (power, ground, or signal)
    Net(String),
    /// Connected to a single component pin
    Pin { reference: String, pin: String },
}

/// A semantic connection endpoint
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticConnection {
    /// Regular pin reference (e.g., "U1:GPIO0")
    Pin { reference: String, pin: String },
    /// Net label (e.g., "&VCC")
    Net(String),
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
}

impl fmt::Display for SemanticConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticConnection::Pin { reference, pin } => write!(f, "{}:{}", reference, pin),
            SemanticConnection::Net(name) => write!(f, "&{}", name),
            SemanticConnection::MergedResistor { reference, value, target } => {
                match target {
                    MergedTarget::Net(net) => {
                        let net_type = NetType::classify(net);
                        if net_type.is_power() {
                            write!(f, "$PULLUP({}, {}, &{})", reference, value, net)
                        } else if net_type.is_ground() {
                            write!(f, "$PULLDOWN({}, {})", reference, value)
                        } else {
                            write!(f, "$RESISTOR({}, {}, &{})", reference, value, net)
                        }
                    }
                    MergedTarget::Pin { reference: pin_ref, pin } => {
                        write!(f, "$RESISTOR({}, {}, {}:{})", reference, value, pin_ref, pin)
                    }
                }
            }
            SemanticConnection::MergedCapacitor { reference, value, polarized, target } => {
                let suffix = if *polarized { ", P" } else { "" };
                match target {
                    MergedTarget::Net(net) => {
                        let net_type = NetType::classify(net);
                        if net_type.is_ground() {
                            write!(f, "$DECAP({}, {}{})", reference, value, suffix)
                        } else {
                            write!(f, "$CAPACITOR({}, {}, &{}{})", reference, value, net, suffix)
                        }
                    }
                    MergedTarget::Pin { reference: pin_ref, pin } => {
                        write!(f, "$CAPACITOR({}, {}, {}:{}{})", reference, value, pin_ref, pin, suffix)
                    }
                }
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

/// The complete semantic outline of a schematic
#[derive(Debug, Clone)]
pub struct SemanticOutline {
    /// Components (excluding absorbed passives)
    pub components: Vec<SemanticComponent>,
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
    description: Option<String>,
    component_type: ComponentType,
}

/// Internal structure for tracking net info during analysis
#[derive(Debug, Clone)]
struct NetData {
    /// Net name (if any)
    name: Option<String>,
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

        // Get pin names from lib_symbol
        let lib_symbol = schematic.lib_symbols.iter().find(|s| s.name == symbol.lib_id);
        let pins: Vec<String> = if let Some(lib_sym) = lib_symbol {
            let mut pin_names: Vec<String> = lib_sym
                .units
                .iter()
                .flat_map(|u| u.pins.iter())
                .map(|p| {
                    // Prefer pin name over number if name is meaningful
                    if p.name.name != "~" && !p.name.name.is_empty() {
                        p.name.name.clone()
                    } else {
                        p.number.number.clone()
                    }
                })
                .collect();
            pin_names.sort();
            pin_names.dedup();
            pin_names
        } else {
            symbol.pins.iter().map(|p| p.number.clone()).collect()
        };

        components.insert(reference.clone(), ComponentData {
            reference,
            lib_id: symbol.lib_id.clone(),
            value,
            pins,
            description,
            component_type,
        });
    }

    // Step 2: Build connectivity using the existing outline logic
    let raw_outline = super::outline::build_outline(schematic);

    // Step 3: Build net data from raw outline
    let mut net_data: Vec<NetData> = Vec::new();
    for net in &raw_outline.nets {
        let pins: Vec<(String, String)> = net
            .connections
            .iter()
            .filter_map(|conn| {
                let parts: Vec<&str> = conn.split(':').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();

        net_data.push(NetData {
            name: Some(net.name.clone()),
            pins,
        });
    }

    // Step 4: Pattern recognition - identify pullups, pulldowns, decoupling caps
    let mut absorbed: HashSet<String> = HashSet::new();
    let mut semantic_nets: Vec<ProcessedNet> = Vec::new();

    for net in &net_data {
        let mut endpoints: Vec<SemanticConnection> = Vec::new();
        let mut absorbed_in_this_net: Vec<String> = Vec::new();

        // Separate pins by component type
        let mut passive_pins: Vec<(&String, &String)> = Vec::new();
        let mut active_pins: Vec<(&String, &String)> = Vec::new();

        for (ref_, pin) in &net.pins {
            if let Some(comp) = components.get(ref_) {
                if comp.component_type.is_two_pin_passive() {
                    passive_pins.push((ref_, pin));
                } else {
                    active_pins.push((ref_, pin));
                }
            }
        }

        // Determine net type for this net
        let net_type = net.name.as_ref().map(|n| NetType::classify(n));

        // Process 2-pin passives for merging
        // Rules for merging:
        // 1. If the OTHER net is power/ground, merge into THIS net (power/ground acts as sink)
        // 2. Otherwise, merge into the "larger" net (the one with more than just passive + 1 other pin)
        for (ref_, _pin) in &passive_pins {
            if absorbed.contains(*ref_) {
                continue;
            }

            let comp = match components.get(*ref_) {
                Some(c) => c,
                None => continue,
            };

            // Get info about the other net this passive connects to
            let other_info = find_other_pin_net_info(&net_data, ref_, net.name.as_deref());

            if let Some(info) = other_info {
                let other_type = NetType::classify(&info.net_name);

                let this_is_power_ground = net_type.map(|t| t.is_power() || t.is_ground()).unwrap_or(false);
                let other_is_power_ground = other_type.is_power() || other_type.is_ground();
                let this_is_ground = net_type.map(|t| t.is_ground()).unwrap_or(false);
                let other_is_ground = other_type.is_ground();

                // Check if we should merge this passive into THIS net
                // Priority: always merge AWAY from power/ground nets, prefer non-GND over non-power
                let should_merge_here = if this_is_power_ground && !other_is_power_ground {
                    // THIS net is power/ground, other is not - don't merge here
                    false
                } else if other_is_power_ground && !this_is_power_ground {
                    // Other net is power/ground - merge into THIS net
                    true
                } else if this_is_ground && other_is_power_ground && !other_is_ground {
                    // Both are power/ground, but THIS is GND and other is power - merge to other
                    false
                } else if other_is_ground && this_is_power_ground && !this_is_ground {
                    // Both are power/ground, but other is GND and THIS is power - merge here
                    true
                } else if let Some(ref _target_str) = info.single_other_pin {
                    // Use "small net" logic - other net has only passive + 1 other pin
                    true
                } else {
                    false
                };

                if should_merge_here {
                    // Determine the target - either a net name or the single other pin
                    let target = if other_type.is_power() || other_type.is_ground() {
                        MergedTarget::Net(info.net_name.clone())
                    } else if let Some(ref target_str) = info.single_other_pin {
                        parse_merged_target(target_str, &info.net_name)
                    } else {
                        continue;
                    };

                    match comp.component_type {
                        ComponentType::Resistor => {
                            endpoints.push(SemanticConnection::MergedResistor {
                                reference: ref_.to_string(),
                                value: comp.value.clone(),
                                target,
                            });
                            absorbed_in_this_net.push(ref_.to_string());
                        }
                        ComponentType::Capacitor { polarized } => {
                            endpoints.push(SemanticConnection::MergedCapacitor {
                                reference: ref_.to_string(),
                                value: comp.value.clone(),
                                polarized,
                                target,
                            });
                            absorbed_in_this_net.push(ref_.to_string());
                        }
                        _ => {}
                    }
                }
            }
        }

        // Add non-absorbed passive pins as regular pin connections
        for (ref_, pin) in &passive_pins {
            if !absorbed.contains(*ref_) && !absorbed_in_this_net.contains(*ref_) {
                // Check if this passive will be absorbed on the OTHER net
                let should_skip = if let Some(info) = find_other_pin_net_info(&net_data, ref_, net.name.as_deref()) {
                    let other_type = NetType::classify(&info.net_name);
                    let this_is_power_ground = net_type.map(|t| t.is_power() || t.is_ground()).unwrap_or(false);
                    let other_is_power_ground = other_type.is_power() || other_type.is_ground();
                    let this_is_ground = net_type.map(|t| t.is_ground()).unwrap_or(false);
                    let other_is_ground = other_type.is_ground();

                    // Skip if this net is power/ground and other is not (will be merged there)
                    if this_is_power_ground && !other_is_power_ground {
                        true
                    }
                    // Skip if this is GND and other is power (will be merged there)
                    else if this_is_ground && other_is_power_ground && !other_is_ground {
                        true
                    }
                    // Skip if this net is "small" (passive + only 1 other pin)
                    else if net.pins.len() == 2 {
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !should_skip {
                    endpoints.push(SemanticConnection::Pin {
                        reference: ref_.to_string(),
                        pin: pin.to_string(),
                    });
                }
            }
        }

        // Add active component pins
        for (ref_, pin) in &active_pins {
            endpoints.push(SemanticConnection::Pin {
                reference: ref_.to_string(),
                pin: pin.to_string(),
            });
        }

        // Add net label
        if let Some(name) = &net.name {
            // Only add net label if it's not an auto-generated name
            if !name.starts_with("NET_") {
                endpoints.push(SemanticConnection::Net(name.clone()));
            }
        }

        // Record absorbed components
        for ref_ in absorbed_in_this_net {
            absorbed.insert(ref_);
        }

        // Only add net if it has at least 2 meaningful endpoints
        // (a single pin alone is not useful - it was likely absorbed into another net)
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
                if let SemanticConnection::Net(name) = ep {
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

    SemanticOutline {
        components: final_components,
        connections: semantic_nets,
        absorbed_components: absorbed,
    }
}

/// Information about a component's connection on another net
struct OtherNetInfo {
    /// Name of the other net
    net_name: String,
    /// If there's exactly one other pin (not this component), its "ref:pin" string
    single_other_pin: Option<String>,
}

/// Parse a merged target from a "ref:pin" string and net name
/// If the target is a power symbol (starts with #), use the net name instead
fn parse_merged_target(pin_str: &str, net_name: &str) -> MergedTarget {
    let parts: Vec<&str> = pin_str.split(':').collect();
    if parts.len() == 2 {
        let reference = parts[0];
        // Power symbols start with # - use the net name instead
        if reference.starts_with('#') {
            MergedTarget::Net(net_name.to_string())
        } else {
            MergedTarget::Pin {
                reference: reference.to_string(),
                pin: parts[1].to_string(),
            }
        }
    } else {
        // Fallback to net name
        MergedTarget::Net(net_name.to_string())
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

        let net = SemanticConnection::Net("VCC".to_string());
        assert_eq!(net.to_string(), "&VCC");

        // Pullup: resistor to power net
        let pullup = SemanticConnection::MergedResistor {
            reference: "R1".to_string(),
            value: "10k".to_string(),
            target: MergedTarget::Net("VCC".to_string()),
        };
        assert_eq!(pullup.to_string(), "$PULLUP(R1, 10k, &VCC)");

        // Pulldown: resistor to ground
        let pulldown = SemanticConnection::MergedResistor {
            reference: "R2".to_string(),
            value: "10k".to_string(),
            target: MergedTarget::Net("GND".to_string()),
        };
        assert_eq!(pulldown.to_string(), "$PULLDOWN(R2, 10k)");

        // Series resistor to a pin
        let series = SemanticConnection::MergedResistor {
            reference: "R3".to_string(),
            value: "100".to_string(),
            target: MergedTarget::Pin { reference: "U1".to_string(), pin: "TX".to_string() },
        };
        assert_eq!(series.to_string(), "$RESISTOR(R3, 100, U1:TX)");

        // Decoupling cap to ground
        let decap = SemanticConnection::MergedCapacitor {
            reference: "C1".to_string(),
            value: "100nF".to_string(),
            polarized: false,
            target: MergedTarget::Net("GND".to_string()),
        };
        assert_eq!(decap.to_string(), "$DECAP(C1, 100nF)");

        // Polarized decoupling cap
        let decap_polar = SemanticConnection::MergedCapacitor {
            reference: "C2".to_string(),
            value: "10uF".to_string(),
            polarized: true,
            target: MergedTarget::Net("GND".to_string()),
        };
        assert_eq!(decap_polar.to_string(), "$DECAP(C2, 10uF, P)");

        // Capacitor to power net (not ground)
        let cap_power = SemanticConnection::MergedCapacitor {
            reference: "C3".to_string(),
            value: "1uF".to_string(),
            polarized: false,
            target: MergedTarget::Net("VCC".to_string()),
        };
        assert_eq!(cap_power.to_string(), "$CAPACITOR(C3, 1uF, &VCC)");
    }
}
