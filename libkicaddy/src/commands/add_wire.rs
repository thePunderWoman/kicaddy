//! AddWireCommand - add a wire connection to a schematic

use crate::commands::{resolve_endpoint, Command, CommandError};
use crate::common::Point;
use crate::schematic::{RoutingMode, Schematic};

/// Specifies one endpoint of a wire
#[derive(Debug, Clone)]
pub enum WireEndpoint {
    /// Connect to a pin on a component (by reference and pin number/name)
    Pin { reference: String, pin: String },
    /// Connect to an explicit point
    Point(Point),
}

impl WireEndpoint {
    /// Create a pin endpoint from reference and pin
    pub fn pin(reference: impl Into<String>, pin: impl Into<String>) -> Self {
        WireEndpoint::Pin {
            reference: reference.into(),
            pin: pin.into(),
        }
    }

    /// Create a point endpoint
    pub fn point(x: f64, y: f64) -> Self {
        WireEndpoint::Point(Point::new(x, y))
    }
}

/// Command to add a wire to a schematic
pub struct AddWireCommand {
    /// Starting endpoint of the wire
    pub from: WireEndpoint,
    /// Ending endpoint of the wire
    pub to: WireEndpoint,
    /// Routing mode for the wire
    pub routing: RoutingMode,
}

/// Result of adding a wire
pub struct AddWireResult {
    /// The resolved start point
    pub from: Point,
    /// The resolved end point
    pub to: Point,
}

impl Command for AddWireCommand {
    type Output = AddWireResult;

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        // Resolve start point
        let start_point = match &self.from {
            WireEndpoint::Pin { reference, pin } => resolve_endpoint(schematic, reference, pin)?,
            WireEndpoint::Point(p) => *p,
        };

        // Resolve end point
        let end_point = match &self.to {
            WireEndpoint::Pin { reference, pin } => resolve_endpoint(schematic, reference, pin)?,
            WireEndpoint::Point(p) => *p,
        };

        // Add the wire with routing
        schematic.add_wire_routed(start_point, end_point, self.routing);

        Ok(AddWireResult {
            from: start_point,
            to: end_point,
        })
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

    fn create_test_schematic_with_resistor() -> Schematic {
        let mut schematic = Schematic::new();

        // Create a symbol with pins
        let lib_symbol = Symbol {
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

        schematic.lib_symbols.push(lib_symbol);

        let symbol_instance = SymbolInstance {
            lib_id: "Device:R".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            power: false,
            fields_autoplaced: true,
            uuid: "test-uuid".to_string(),
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

        schematic.symbols.push(symbol_instance);
        schematic
    }

    #[test]
    fn test_add_wire_point_to_point() {
        let mut schematic = Schematic::new();

        let cmd = AddWireCommand {
            from: WireEndpoint::point(100.0, 50.0),
            to: WireEndpoint::point(150.0, 50.0),
            routing: RoutingMode::Direct,
        };

        let result = cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.wires.len(), 1);
        assert_eq!(result.from.x, 100.0);
        assert_eq!(result.to.x, 150.0);
    }

    #[test]
    fn test_add_wire_pin_to_point() {
        let mut schematic = create_test_schematic_with_resistor();

        let cmd = AddWireCommand {
            from: WireEndpoint::pin("R1", "1"),
            to: WireEndpoint::point(150.0, 50.0),
            routing: RoutingMode::Direct,
        };

        let result = cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.wires.len(), 1);
        // Start point should be at pin position
        assert!((result.from.x - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_add_wire_symbol_not_found() {
        let mut schematic = Schematic::new();

        let cmd = AddWireCommand {
            from: WireEndpoint::pin("R99", "1"),
            to: WireEndpoint::point(150.0, 50.0),
            routing: RoutingMode::Direct,
        };

        let result = cmd.execute(&mut schematic);
        assert!(matches!(result, Err(CommandError::SymbolNotFound(_))));
    }

    #[test]
    fn test_add_wire_pin_not_found() {
        let mut schematic = create_test_schematic_with_resistor();

        let cmd = AddWireCommand {
            from: WireEndpoint::pin("R1", "99"), // Non-existent pin
            to: WireEndpoint::point(150.0, 50.0),
            routing: RoutingMode::Direct,
        };

        let result = cmd.execute(&mut schematic);
        assert!(matches!(result, Err(CommandError::PinNotFound { .. })));
    }
}
