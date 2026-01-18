//! Disconnect command - remove connections between endpoints
//!
//! # Usage
//! ```ignore
//! disconnect(["R1:1", "C1:2"])  // remove wire between two pins
//! ```

use crate::commands::{Command, CommandError};
use crate::common::Point;
use crate::connectivity::ConnectionEndpoint;
use crate::schematic::Schematic;

/// Command to disconnect two endpoints
pub struct DisconnectCommand {
    /// First endpoint
    pub endpoint1: String,
    /// Second endpoint
    pub endpoint2: String,
}

/// Result of disconnect operation
#[derive(Debug, Clone)]
pub struct DisconnectOutput {
    /// Number of wires removed
    pub wires_removed: usize,
    /// Number of junctions removed
    pub junctions_removed: usize,
    /// Number of labels removed
    pub labels_removed: usize,
}

impl Command for DisconnectCommand {
    type Output = DisconnectOutput;

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        // Parse endpoints
        let ep1 = ConnectionEndpoint::parse(&self.endpoint1)
            .map_err(|e| CommandError::Other(e.to_string()))?;
        let ep2 = ConnectionEndpoint::parse(&self.endpoint2)
            .map_err(|e| CommandError::Other(e.to_string()))?;

        // Resolve positions
        let pos1 = resolve_position(&ep1, schematic)?;
        let pos2 = resolve_position(&ep2, schematic)?;

        let mut junctions_removed = 0;
        let mut labels_removed = 0;

        // Find and remove wires connecting these positions
        let tolerance: f64 = 1.27; // One grid unit
        let initial_wire_count = schematic.wires.len();

        schematic.wires.retain(|wire| {
            if wire.points.len() < 2 {
                return true;
            }

            let first = wire.points[0];
            let last = wire.points[wire.points.len() - 1];

            // Check if wire connects these two positions (in either direction)
            let connects = (points_near(first, pos1, tolerance) && points_near(last, pos2, tolerance))
                || (points_near(first, pos2, tolerance) && points_near(last, pos1, tolerance));

            // Also check if wire passes through both positions
            let passes_through_both = wire_passes_through(wire, pos1, tolerance)
                && wire_passes_through(wire, pos2, tolerance);

            !connects && !passes_through_both
        });

        let wires_removed = initial_wire_count - schematic.wires.len();

        // If we removed wires, clean up orphaned junctions
        if wires_removed > 0 {
            let initial_junction_count = schematic.junctions.len();

            // First, collect pin positions to avoid borrow issues
            let pin_positions: Vec<Point> = schematic.symbols.iter()
                .flat_map(|sym| {
                    sym.pins.iter().filter_map(|pin| {
                        schematic.get_pin_position(sym, &pin.number).map(|(p, _)| p)
                    })
                })
                .collect();

            // Clone wires for checking
            let wires_snapshot: Vec<Vec<Point>> = schematic.wires.iter()
                .map(|w| w.points.clone())
                .collect();

            // Remove junctions that are no longer at wire intersections
            schematic.junctions.retain(|junction| {
                let jp = junction.position;

                // Count wires connected to this junction
                let wire_count = wires_snapshot.iter().filter(|points| {
                    points.iter().any(|p| points_near(*p, jp, 0.1))
                }).count();

                // Check if junction is at a pin position
                let at_pin = pin_positions.iter().any(|pp| points_near(*pp, jp, 0.1));

                // Keep junction if 3+ wires still meet here, or if it's at a pin
                wire_count >= 3 || at_pin
            });

            junctions_removed = initial_junction_count - schematic.junctions.len();
        }

        // Handle net label disconnection
        if let ConnectionEndpoint::Net(net_name) = &ep1 {
            let pos = pos2;
            if schematic.delete_label(net_name, Some(pos)) {
                labels_removed += 1;
            }
        }
        if let ConnectionEndpoint::Net(net_name) = &ep2 {
            let pos = pos1;
            if schematic.delete_label(net_name, Some(pos)) {
                labels_removed += 1;
            }
        }

        if wires_removed == 0 && labels_removed == 0 {
            return Err(CommandError::Other(
                "No connection found between the specified endpoints".to_string(),
            ));
        }

        Ok(DisconnectOutput {
            wires_removed,
            junctions_removed,
            labels_removed,
        })
    }
}

/// Resolve an endpoint to a position
fn resolve_position(ep: &ConnectionEndpoint, schematic: &Schematic) -> Result<Point, CommandError> {
    match ep {
        ConnectionEndpoint::Pin { reference, pin } => {
            let symbol = schematic
                .find_symbol_by_reference(reference)
                .ok_or_else(|| CommandError::SymbolNotFound(reference.clone()))?;

            let (pos, _) = schematic.get_pin_position(symbol, pin).ok_or_else(|| {
                CommandError::PinNotFound {
                    reference: reference.clone(),
                    pin: pin.clone(),
                }
            })?;

            Ok(pos)
        }
        ConnectionEndpoint::Net(name) => {
            // For net labels, find the label position
            // Check local labels first
            if let Some(label) = schematic.labels.iter().find(|l| l.text == *name) {
                return Ok(Point::new(label.position.x, label.position.y));
            }
            // Check global labels
            if let Some(label) = schematic.global_labels.iter().find(|l| l.text == *name) {
                return Ok(Point::new(label.position.x, label.position.y));
            }
            Err(CommandError::Other(format!("Net label '{}' not found", name)))
        }
    }
}

/// Check if two points are near each other
fn points_near(p1: Point, p2: Point, tolerance: f64) -> bool {
    let dx = p1.x - p2.x;
    let dy = p1.y - p2.y;
    dx * dx + dy * dy < tolerance * tolerance
}

/// Check if a wire passes through or near a point
fn wire_passes_through(wire: &crate::schematic::Wire, pos: Point, tolerance: f64) -> bool {
    // Check endpoints
    for p in &wire.points {
        if points_near(*p, pos, tolerance) {
            return true;
        }
    }

    // Check segments
    for i in 0..wire.points.len().saturating_sub(1) {
        let p1 = wire.points[i];
        let p2 = wire.points[i + 1];
        if point_near_segment(pos, p1, p2, tolerance) {
            return true;
        }
    }

    false
}

/// Check if a point is near a line segment
fn point_near_segment(point: Point, p1: Point, p2: Point, tolerance: f64) -> bool {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.0001 {
        // Segment is essentially a point
        return points_near(point, p1, tolerance);
    }

    // Project point onto line, clamped to segment
    let t = ((point.x - p1.x) * dx + (point.y - p1.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let closest = Point::new(p1.x + t * dx, p1.y + t * dy);
    points_near(point, closest, tolerance)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Position, Property, Stroke};
    use crate::schematic::{SymbolInstance, Wire};
    use crate::symbol::{
        Pin, PinElectricalType, PinGraphicStyle, PinName, PinNumber, Symbol, SymbolUnit,
    };

    fn create_test_schematic_with_wire() -> Schematic {
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

        // Add R1 at (100, 50)
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

        // Add R2 at (120, 50)
        let r2 = SymbolInstance {
            lib_id: "Device:R".to_string(),
            position: Position::new(120.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: true,
            uuid: "r2-uuid".to_string(),
            properties: vec![Property {
                name: "Reference".to_string(),
                value: "R2".to_string(),
                position: None,
                effects: None,
            }],
            pins: vec![],
            instances: vec![],
            mirror: None,
        };

        schematic.symbols.push(r1);
        schematic.symbols.push(r2);

        // Add wire between R1:1 and R2:1 (both at y=45.0 since pins point up from y=50)
        // R1 pin 1 is at (100, 50-2.54-2.54) = (100, ~45) - need to check actual position
        // Actually let's just add a wire at known coordinates
        schematic.wires.push(Wire {
            points: vec![
                Point::new(100.0, 45.0),
                Point::new(120.0, 45.0),
            ],
            stroke: Stroke::default(),
            uuid: "wire-uuid".to_string(),
        });

        schematic
    }

    #[test]
    fn test_disconnect_two_pins() {
        let mut schematic = create_test_schematic_with_wire();
        assert_eq!(schematic.wires.len(), 1);

        // Get actual pin positions for the wire endpoints
        let r1 = schematic.find_symbol_by_reference("R1").unwrap();
        let r2 = schematic.find_symbol_by_reference("R2").unwrap();
        let (r1_pin1_pos, _) = schematic.get_pin_position(r1, "1").unwrap();
        let (r2_pin1_pos, _) = schematic.get_pin_position(r2, "1").unwrap();

        // Update wire to connect actual pin positions
        schematic.wires[0].points = vec![r1_pin1_pos, r2_pin1_pos];

        let cmd = DisconnectCommand {
            endpoint1: "R1:1".to_string(),
            endpoint2: "R2:1".to_string(),
        };

        let result = cmd.execute(&mut schematic).unwrap();
        assert_eq!(result.wires_removed, 1);
        assert_eq!(schematic.wires.len(), 0);
    }

    #[test]
    fn test_disconnect_no_connection() {
        let mut schematic = create_test_schematic_with_wire();

        let cmd = DisconnectCommand {
            endpoint1: "R1:2".to_string(),  // Different pin
            endpoint2: "R2:2".to_string(),
        };

        let result = cmd.execute(&mut schematic);
        assert!(result.is_err());
    }

    #[test]
    fn test_disconnect_invalid_component() {
        let mut schematic = create_test_schematic_with_wire();

        let cmd = DisconnectCommand {
            endpoint1: "R99:1".to_string(),
            endpoint2: "R2:1".to_string(),
        };

        let result = cmd.execute(&mut schematic);
        assert!(matches!(result, Err(CommandError::SymbolNotFound(_))));
    }
}
