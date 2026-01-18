//! Connect command - create logical connections between pins and nets
//!
//! # Usage
//! ```ignore
//! connect(["R1:1", "C1:2"])           // wire between two pins
//! connect(["R1:1", "C1:2", "U1:10"])  // star connection with junction
//! connect(["U1:1", "&GND"])           // pin to net via label
//! connect(["U1:1", "R1:1", "&GND"])   // multiple pins to net
//! ```

use crate::commands::{snap_point_to_grid, Command, CommandError};
use crate::common::{Point, Position};
use crate::connectivity::{ConnectionEndpoint, ConnectionError};
use crate::schematic::{LabelShape, RoutingMode, Schematic};

/// Command to connect multiple endpoints together
pub struct ConnectCommand {
    /// Endpoints to connect (minimum 2)
    pub endpoints: Vec<String>,
}

/// Result of connect operation
#[derive(Debug, Clone)]
pub struct ConnectOutput {
    /// Number of wires created
    pub wires_created: usize,
    /// Number of junctions created
    pub junctions_created: usize,
    /// Labels created (empty if none)
    pub labels_created: Vec<String>,
    /// Net name if connecting to a net
    pub net_name: Option<String>,
}

impl Command for ConnectCommand {
    type Output = ConnectOutput;

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        if self.endpoints.len() < 2 {
            return Err(CommandError::Other(
                "At least 2 endpoints are required for a connection".to_string(),
            ));
        }

        // Parse all endpoints
        let parsed: Vec<ConnectionEndpoint> = self
            .endpoints
            .iter()
            .map(|s| ConnectionEndpoint::parse(s))
            .collect::<Result<Vec<_>, ConnectionError>>()
            .map_err(|e| CommandError::Other(e.to_string()))?;

        // Separate pins and nets
        let mut pin_endpoints: Vec<(&str, &str)> = Vec::new();
        let mut net_name: Option<String> = None;

        for ep in &parsed {
            match ep {
                ConnectionEndpoint::Pin { reference, pin } => {
                    pin_endpoints.push((reference, pin));
                }
                ConnectionEndpoint::Net(name) => {
                    // Only one net per connection
                    if net_name.is_some() {
                        return Err(CommandError::Other(
                            "Only one net label allowed per connection".to_string(),
                        ));
                    }
                    net_name = Some(name.clone());
                }
            }
        }

        if pin_endpoints.is_empty() {
            return Err(CommandError::Other(
                "At least one pin endpoint is required".to_string(),
            ));
        }

        // Resolve pin positions and angles
        let mut pin_positions: Vec<(Point, f64)> = Vec::new();
        for (reference, pin) in &pin_endpoints {
            let symbol = schematic
                .find_symbol_by_reference(reference)
                .ok_or_else(|| CommandError::SymbolNotFound((*reference).to_string()))?;

            let (pos, angle) = schematic
                .get_pin_position(symbol, pin)
                .ok_or_else(|| CommandError::PinNotFound {
                    reference: (*reference).to_string(),
                    pin: (*pin).to_string(),
                })?;

            pin_positions.push((pos, angle));
        }

        let mut wires_created = 0;
        let mut junctions_created = 0;
        let mut labels_created: Vec<String> = Vec::new();

        // Determine connection strategy
        if let Some(ref net) = net_name {
            // Connecting to a net - add label at each pin position (no wires needed)
            let is_power = ConnectionEndpoint::Net(net.clone()).is_power_net();

            for (pin_pos, pin_angle) in &pin_positions {
                if is_power {
                    schematic.add_global_label(
                        net,
                        Position::new(pin_pos.x, pin_pos.y, label_angle_from_pin(*pin_angle)),
                        LabelShape::Passive,
                    );
                } else {
                    schematic.add_label(
                        net,
                        Position::new(pin_pos.x, pin_pos.y, label_angle_from_pin(*pin_angle)),
                    );
                }
            }
            labels_created.push(net.clone());
        } else if pin_positions.len() == 2 {
            // Two pins, no net - direct wire
            let (from_pos, _) = pin_positions[0];
            let (to_pos, _) = pin_positions[1];
            schematic.add_wire_routed(from_pos, to_pos, RoutingMode::Orthogonal);
            wires_created = 1;
        } else {
            // Multiple pins, no net - star connection from first pin
            let (junction_pos, _) = pin_positions[0];
            let junction_pos = snap_point_to_grid(junction_pos);

            // Add junction at first pin position (where all wires meet)
            if pin_positions.len() > 2 {
                schematic.add_junction(junction_pos);
                junctions_created = 1;
            }

            // Wire from first pin to each other pin
            for (pos, _) in pin_positions.iter().skip(1) {
                schematic.add_wire_routed(junction_pos, *pos, RoutingMode::Orthogonal);
                wires_created += 1;
            }
        }

        Ok(ConnectOutput {
            wires_created,
            junctions_created,
            labels_created,
            net_name,
        })
    }
}

/// Calculate label angle from pin direction
/// Labels should point away from the pin direction
fn label_angle_from_pin(pin_angle: f64) -> f64 {
    // Pin angle 0 = pointing right (wire goes right)
    // Label should face the direction the wire goes
    match (pin_angle as i32) % 360 {
        0 => 0.0,     // Pin points right, label faces right
        90 => 90.0,   // Pin points up, label faces up
        180 => 180.0, // Pin points left, label faces left
        270 => 270.0, // Pin points down, label faces down
        a if a < 0 => label_angle_from_pin((a + 360) as f64),
        _ => pin_angle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Position, Property};
    use crate::schematic::SymbolInstance;
    use crate::symbol::{
        Pin, PinElectricalType, PinGraphicStyle, PinName, PinNumber, Symbol, SymbolUnit,
    };

    fn create_test_schematic() -> Schematic {
        let mut schematic = Schematic::new();

        // Create resistor symbol
        let resistor = Symbol {
            name: "Device:R".to_string(),
            extends: None,
            pin_numbers_hide: false,
            pin_names_offset: 0.0,
            pin_names_hide: false,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            properties: vec![],
            units: vec![SymbolUnit {
                name: "R_1_1".to_string(),
                graphics: vec![],
                pins: vec![
                    Pin {
                        electrical_type: PinElectricalType::Passive,
                        graphic_style: PinGraphicStyle::Line,
                        position: Position::new(0.0, 2.54, 270.0),
                        length: 2.54,
                        name: PinName {
                            name: "~".to_string(),
                            effects: None,
                        },
                        number: PinNumber {
                            number: "1".to_string(),
                            effects: None,
                        },
                        hide: false,
                    },
                    Pin {
                        electrical_type: PinElectricalType::Passive,
                        graphic_style: PinGraphicStyle::Line,
                        position: Position::new(0.0, -2.54, 90.0),
                        length: 2.54,
                        name: PinName {
                            name: "~".to_string(),
                            effects: None,
                        },
                        number: PinNumber {
                            number: "2".to_string(),
                            effects: None,
                        },
                        hide: false,
                    },
                ],
            }],
            embedded_fonts: None,
        };

        schematic.lib_symbols.push(resistor.clone());

        // Add R1
        let r1 = SymbolInstance {
            lib_id: "Device:R".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: true,
            uuid: "r1-uuid".to_string(),
            properties: vec![Property {
                name: "Reference".to_string(),
                value: "R1".to_string(),
                position: None,
                effects: None,
            }],
            pins: vec![],
            instances: vec![],
            mirror: None,
        };

        // Add C1
        let c1 = SymbolInstance {
            lib_id: "Device:R".to_string(),
            position: Position::new(120.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: true,
            uuid: "c1-uuid".to_string(),
            properties: vec![Property {
                name: "Reference".to_string(),
                value: "C1".to_string(),
                position: None,
                effects: None,
            }],
            pins: vec![],
            instances: vec![],
            mirror: None,
        };

        schematic.symbols.push(r1);
        schematic.symbols.push(c1);

        schematic
    }

    #[test]
    fn test_connect_two_pins() {
        let mut schematic = create_test_schematic();
        let initial_wire_count = schematic.wires.len();

        let cmd = ConnectCommand {
            endpoints: vec!["R1:1".to_string(), "C1:1".to_string()],
        };

        let result = cmd.execute(&mut schematic).unwrap();
        assert_eq!(result.wires_created, 1);
        assert_eq!(result.junctions_created, 0);
        assert!(result.labels_created.is_empty());
        assert_eq!(schematic.wires.len(), initial_wire_count + 1);
    }

    #[test]
    fn test_connect_pin_to_net() {
        let mut schematic = create_test_schematic();
        let initial_label_count = schematic.global_labels.len();

        let cmd = ConnectCommand {
            endpoints: vec!["R1:2".to_string(), "&GND".to_string()],
        };

        let result = cmd.execute(&mut schematic).unwrap();
        assert_eq!(result.wires_created, 0);
        assert_eq!(result.labels_created.len(), 1);
        assert_eq!(result.labels_created[0], "GND");
        // GND is a power net, should create global label
        assert_eq!(schematic.global_labels.len(), initial_label_count + 1);
    }

    #[test]
    fn test_connect_insufficient_endpoints() {
        let mut schematic = create_test_schematic();

        let cmd = ConnectCommand {
            endpoints: vec!["R1:1".to_string()],
        };

        let result = cmd.execute(&mut schematic);
        assert!(result.is_err());
    }

    #[test]
    fn test_connect_invalid_pin() {
        let mut schematic = create_test_schematic();

        let cmd = ConnectCommand {
            endpoints: vec!["R1:99".to_string(), "C1:1".to_string()],
        };

        let result = cmd.execute(&mut schematic);
        assert!(result.is_err());
    }

    #[test]
    fn test_connect_invalid_component() {
        let mut schematic = create_test_schematic();

        let cmd = ConnectCommand {
            endpoints: vec!["R99:1".to_string(), "C1:1".to_string()],
        };

        let result = cmd.execute(&mut schematic);
        assert!(matches!(result, Err(CommandError::SymbolNotFound(_))));
    }
}
