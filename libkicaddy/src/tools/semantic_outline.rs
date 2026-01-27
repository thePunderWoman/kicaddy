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

/// A semantic connection endpoint
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticConnection {
    /// Regular pin reference (e.g., "U1:GPIO0")
    Pin { reference: String, pin: String },
    /// Net label (e.g., "&VCC")
    Net(String),
    /// Pullup resistor pattern: $PULLUP(R2, &VCC, 10k)
    Pullup {
        resistor_ref: String,
        power_net: String,
        value: String,
    },
    /// Pulldown resistor pattern: $PULLDOWN(R3, 10k) - ground is implied
    Pulldown {
        resistor_ref: String,
        value: String,
    },
    /// Decoupling capacitor pattern: $DECAP(C1, 100nF) or $DECAP(C1, 100nF, P) for polarized
    DecouplingCap {
        cap_ref: String,
        value: String,
        polarized: bool,
    },
}

impl fmt::Display for SemanticConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticConnection::Pin { reference, pin } => write!(f, "{}:{}", reference, pin),
            SemanticConnection::Net(name) => write!(f, "&{}", name),
            SemanticConnection::Pullup { resistor_ref, power_net, value } => {
                write!(f, "$PULLUP({}, &{}, {})", resistor_ref, power_net, value)
            }
            SemanticConnection::Pulldown { resistor_ref, value } => {
                write!(f, "$PULLDOWN({}, {})", resistor_ref, value)
            }
            SemanticConnection::DecouplingCap { cap_ref, value, polarized } => {
                if *polarized {
                    write!(f, "$DECAP({}, {}, P)", cap_ref, value)
                } else {
                    write!(f, "$DECAP({}, {})", cap_ref, value)
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
        lines.push(String::new());

        for comp in &self.components {
            lines.push(format!("{} - {}", comp.reference, comp.lib_id));
            if !comp.pins.is_empty() {
                lines.push(format!("  Pins: {}", comp.pins.join(", ")));
            }
            if !comp.value.is_empty() && comp.value != comp.reference {
                lines.push(format!("  Value: {}", comp.value));
            }
            if let Some(desc) = &comp.description {
                lines.push(format!("  Label: {}", desc));
            }
            lines.push(String::new());
        }

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

        // Determine net type
        let net_type = net.name.as_ref().map(|n| NetType::classify(n));

        // Check for decoupling caps (capacitor between power and ground)
        // A decoupling cap is part of BOTH a power net and a ground net
        // We need to detect it by looking at what the capacitor connects to

        // Process passive components for semantic patterns
        for (ref_, _pin) in &passive_pins {
            if absorbed.contains(*ref_) {
                continue;
            }

            let comp = match components.get(*ref_) {
                Some(c) => c,
                None => continue,
            };

            // Get the other pin of this 2-pin passive
            let other_pin_net = find_other_pin_net(&net_data, ref_, net.name.as_deref());

            match comp.component_type {
                ComponentType::Resistor => {
                    // Check for pullup: one pin to power, other to signal
                    if let Some(other_net) = &other_pin_net {
                        let other_type = NetType::classify(other_net);

                        if net_type == Some(NetType::Power) && other_type == NetType::Signal {
                            // This net is power, other is signal - absorb as pullup on the signal net
                            // Will be added when processing the signal net
                        } else if net_type == Some(NetType::Signal) && other_type == NetType::Power {
                            // This net is signal, other is power - add pullup here
                            endpoints.push(SemanticConnection::Pullup {
                                resistor_ref: ref_.to_string(),
                                power_net: other_net.clone(),
                                value: comp.value.clone(),
                            });
                            absorbed_in_this_net.push(ref_.to_string());
                        } else if net_type == Some(NetType::Ground) && other_type == NetType::Signal {
                            // This net is ground, other is signal - absorb as pulldown on signal net
                            // Will be added when processing the signal net
                        } else if net_type == Some(NetType::Signal) && other_type == NetType::Ground {
                            // This net is signal, other is ground - add pulldown here
                            endpoints.push(SemanticConnection::Pulldown {
                                resistor_ref: ref_.to_string(),
                                value: comp.value.clone(),
                            });
                            absorbed_in_this_net.push(ref_.to_string());
                        }
                        // For resistors between two signals, don't absorb
                    }
                }
                ComponentType::Capacitor { polarized } => {
                    // Check for decoupling cap: any cap with one pin to ground
                    if let Some(other_net) = &other_pin_net {
                        let other_type = NetType::classify(other_net);

                        if other_type == NetType::Ground && net_type != Some(NetType::Ground) {
                            // Other pin is ground, current net is NOT ground
                            // This cap decouples the current net - add annotation and absorb
                            endpoints.push(SemanticConnection::DecouplingCap {
                                cap_ref: ref_.to_string(),
                                value: comp.value.clone(),
                                polarized,
                            });
                            absorbed_in_this_net.push(ref_.to_string());
                        }
                        // Don't absorb on the ground side - let the non-ground side handle it
                        // This ensures the annotation gets added before the component is marked absorbed
                    }
                }
                _ => {}
            }
        }

        // Add non-absorbed passive pins as regular pin connections
        for (ref_, pin) in &passive_pins {
            if !absorbed.contains(*ref_) && !absorbed_in_this_net.contains(*ref_) {
                // Check if this passive should be skipped because it will be absorbed elsewhere
                let comp = components.get(*ref_);
                let should_skip = if let Some(c) = comp {
                    if let Some(other_net) = find_other_pin_net(&net_data, ref_, net.name.as_deref()) {
                        let other_type = NetType::classify(&other_net);

                        match c.component_type {
                            // Cap on ground net with other pin on non-ground → will be $DECAP
                            ComponentType::Capacitor { .. } if net_type == Some(NetType::Ground) => {
                                other_type != NetType::Ground
                            }
                            // Resistor on ground net with other pin on signal → will be $PULLDOWN
                            ComponentType::Resistor if net_type == Some(NetType::Ground) => {
                                other_type == NetType::Signal
                            }
                            // Resistor on power net with other pin on signal → will be $PULLUP
                            ComponentType::Resistor if net_type == Some(NetType::Power) => {
                                other_type == NetType::Signal
                            }
                            _ => false
                        }
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

        // Only add net if it has meaningful content
        if endpoints.len() >= 2 || endpoints.iter().any(|e| !matches!(e, SemanticConnection::Net(_))) {
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

/// Find the net that the other pin of a 2-pin component is connected to
fn find_other_pin_net(nets: &[NetData], reference: &str, current_net: Option<&str>) -> Option<String> {
    for net in nets {
        // Skip the current net
        if net.name.as_deref() == current_net {
            continue;
        }

        // Check if this component has a pin in this net
        for (ref_, _pin) in &net.pins {
            if ref_ == reference {
                return net.name.clone();
            }
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

        let pullup = SemanticConnection::Pullup {
            resistor_ref: "R1".to_string(),
            power_net: "VCC".to_string(),
            value: "10k".to_string(),
        };
        assert_eq!(pullup.to_string(), "$PULLUP(R1, &VCC, 10k)");

        let pulldown = SemanticConnection::Pulldown {
            resistor_ref: "R2".to_string(),
            value: "10k".to_string(),
        };
        assert_eq!(pulldown.to_string(), "$PULLDOWN(R2, 10k)");

        let decap = SemanticConnection::DecouplingCap {
            cap_ref: "C1".to_string(),
            value: "100nF".to_string(),
            polarized: false,
        };
        assert_eq!(decap.to_string(), "$DECAP(C1, 100nF)");

        let decap_polar = SemanticConnection::DecouplingCap {
            cap_ref: "C2".to_string(),
            value: "10uF".to_string(),
            polarized: true,
        };
        assert_eq!(decap_polar.to_string(), "$DECAP(C2, 10uF, P)");
    }
}
