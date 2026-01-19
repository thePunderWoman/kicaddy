//! Integration tests for the Command pattern

use libkicaddy::commands::{
    parse_label_shape, parse_pin_ref, parse_routing_mode, AddLabelCommand, AddWireCommand,
    Command, CommandError, DeleteComponentCommand, DeleteLabelCommand, DeleteWireCommand,
    LabelLocation, PlaceComponentCommand, WireEndpoint, GRID,
};
use libkicaddy::common::{Point, Position, Property};
use libkicaddy::schematic::{LabelShape, RoutingMode, Schematic};
use libkicaddy::symbol::{
    Pin, PinElectricalType, PinGraphicStyle, PinName, PinNumber, Symbol, SymbolUnit,
};

/// Create a test resistor symbol definition
fn create_resistor_symbol() -> Symbol {
    Symbol {
        name: "R".to_string(),
        extends: None,
        pin_numbers_hide: false,
        pin_names_offset: 0.0,
        pin_names_hide: true,
        exclude_from_sim: false,
        in_bom: true,
        on_board: true,
        properties: vec![
            Property {
                name: "Reference".to_string(),
                value: "R".to_string(),
                position: Some(Position::new(0.0, 2.54, 0.0)),
                effects: None,
            },
            Property {
                name: "Value".to_string(),
                value: "R".to_string(),
                position: Some(Position::new(0.0, -2.54, 0.0)),
                effects: None,
            },
        ],
        units: vec![SymbolUnit {
            name: "R_0_1".to_string(),
            graphics: vec![],
            pins: vec![
                Pin {
                    electrical_type: PinElectricalType::Passive,
                    graphic_style: PinGraphicStyle::Line,
                    position: Position::new(0.0, 3.81, 270.0),
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
                    position: Position::new(0.0, -3.81, 90.0),
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
    }
}

// ============================================================================
// PlaceComponentCommand Tests
// ============================================================================

#[test]
fn test_place_component_command() {
    let mut schematic = Schematic::new();
    let symbol = create_resistor_symbol();

    let cmd = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: Some("R1".to_string()),
        value: Some("10k".to_string()),
    };

    let result = cmd.execute(&mut schematic).unwrap();

    assert_eq!(result.reference, "R1");
    assert_eq!(schematic.symbols.len(), 1);
    assert_eq!(schematic.lib_symbols.len(), 1);
    assert_eq!(schematic.lib_symbols[0].name, "Device:R");
}

#[test]
fn test_place_component_snaps_to_grid() {
    let mut schematic = Schematic::new();
    let symbol = create_resistor_symbol();

    // Position not on grid
    let cmd = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(100.5, 50.3, 0.0),
        reference: Some("R1".to_string()),
        value: None,
    };

    let result = cmd.execute(&mut schematic).unwrap();

    assert!(result.was_snapped);
    // Should snap to 1.27mm grid
    assert!((result.x % GRID).abs() < 0.001 || (GRID - (result.x % GRID).abs()) < 0.001);
    assert!((result.y % GRID).abs() < 0.001 || (GRID - (result.y % GRID).abs()) < 0.001);
}

#[test]
fn test_place_component_auto_reference() {
    let mut schematic = Schematic::new();
    let symbol = create_resistor_symbol();

    let cmd = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: None,
        value: None,
    };

    let result = cmd.execute(&mut schematic).unwrap();

    // Should auto-generate unique reference starting from R1
    assert_eq!(result.reference, "R1");
}

#[test]
fn test_place_multiple_same_symbol() {
    let mut schematic = Schematic::new();
    let symbol = create_resistor_symbol();

    // Place first
    let cmd1 = PlaceComponentCommand {
        symbol: symbol.clone(),
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: Some("R1".to_string()),
        value: None,
    };
    cmd1.execute(&mut schematic).unwrap();

    // Place second
    let cmd2 = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(120.0, 50.0, 0.0),
        reference: Some("R2".to_string()),
        value: None,
    };
    cmd2.execute(&mut schematic).unwrap();

    // Should have 2 instances but only 1 lib_symbol
    assert_eq!(schematic.symbols.len(), 2);
    assert_eq!(schematic.lib_symbols.len(), 1);
}

// ============================================================================
// AddWireCommand Tests
// ============================================================================

#[test]
fn test_add_wire_point_to_point() {
    let mut schematic = Schematic::new();

    let cmd = AddWireCommand {
        from: WireEndpoint::Point(Point::new(100.0, 50.0)),
        to: WireEndpoint::Point(Point::new(150.0, 50.0)),
        routing: RoutingMode::Direct,
    };

    let result = cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.wires.len(), 1);
    assert_eq!(result.from.x, 100.0);
    assert_eq!(result.to.x, 150.0);
}

#[test]
fn test_add_wire_pin_endpoint() {
    let mut schematic = Schematic::new();

    // First place a component
    let symbol = create_resistor_symbol();
    let place_cmd = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: Some("R1".to_string()),
        value: None,
    };
    place_cmd.execute(&mut schematic).unwrap();

    // Now add wire from pin
    let wire_cmd = AddWireCommand {
        from: WireEndpoint::Pin {
            reference: "R1".to_string(),
            pin: "1".to_string(),
        },
        to: WireEndpoint::Point(Point::new(150.0, 50.0)),
        routing: RoutingMode::Direct,
    };

    let result = wire_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.wires.len(), 1);
    // Start point should be at the pin position (near the component)
    assert!((result.from.x - 100.0).abs() < 5.0);
}

#[test]
fn test_add_wire_symbol_not_found() {
    let mut schematic = Schematic::new();

    let cmd = AddWireCommand {
        from: WireEndpoint::Pin {
            reference: "R99".to_string(),
            pin: "1".to_string(),
        },
        to: WireEndpoint::Point(Point::new(150.0, 50.0)),
        routing: RoutingMode::Direct,
    };

    let result = cmd.execute(&mut schematic);
    assert!(matches!(result, Err(CommandError::SymbolNotFound(_))));
}

#[test]
fn test_add_wire_pin_not_found() {
    let mut schematic = Schematic::new();

    // Place component
    let symbol = create_resistor_symbol();
    let place_cmd = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: Some("R1".to_string()),
        value: None,
    };
    place_cmd.execute(&mut schematic).unwrap();

    // Try to add wire to non-existent pin
    let wire_cmd = AddWireCommand {
        from: WireEndpoint::Pin {
            reference: "R1".to_string(),
            pin: "99".to_string(),
        },
        to: WireEndpoint::Point(Point::new(150.0, 50.0)),
        routing: RoutingMode::Direct,
    };

    let result = wire_cmd.execute(&mut schematic);
    assert!(matches!(result, Err(CommandError::PinNotFound { .. })));
}

// ============================================================================
// AddLabelCommand Tests
// ============================================================================

#[test]
fn test_add_local_label() {
    let mut schematic = Schematic::new();

    let cmd = AddLabelCommand {
        text: "NET1".to_string(),
        location: LabelLocation::Position(Position::new(100.0, 50.0, 0.0)),
        global: false,
        shape: LabelShape::Input,
    };

    let result = cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.labels.len(), 1);
    assert_eq!(result.text, "NET1");
    assert!(!result.global);
}

#[test]
fn test_add_global_label() {
    let mut schematic = Schematic::new();

    let cmd = AddLabelCommand {
        text: "VCC".to_string(),
        location: LabelLocation::Position(Position::new(100.0, 50.0, 0.0)),
        global: true,
        shape: LabelShape::Input,
    };

    let result = cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.global_labels.len(), 1);
    assert_eq!(result.text, "VCC");
    assert!(result.global);
    assert_eq!(schematic.global_labels[0].shape, LabelShape::Input);
}

#[test]
fn test_add_label_at_pin() {
    let mut schematic = Schematic::new();

    // Place component
    let symbol = create_resistor_symbol();
    let place_cmd = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: Some("R1".to_string()),
        value: None,
    };
    place_cmd.execute(&mut schematic).unwrap();

    // Add label at pin
    let label_cmd = AddLabelCommand {
        text: "SIGNAL".to_string(),
        location: LabelLocation::Pin {
            reference: "R1".to_string(),
            pin: "1".to_string(),
        },
        global: false,
        shape: LabelShape::Input,
    };

    let result = label_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.labels.len(), 1);
    // Label should be at pin position (near the component)
    assert!((result.position.x - 100.0).abs() < 5.0);
}

#[test]
fn test_add_label_symbol_not_found() {
    let mut schematic = Schematic::new();

    let cmd = AddLabelCommand {
        text: "NET".to_string(),
        location: LabelLocation::Pin {
            reference: "R99".to_string(),
            pin: "1".to_string(),
        },
        global: false,
        shape: LabelShape::Input,
    };

    let result = cmd.execute(&mut schematic);
    assert!(matches!(result, Err(CommandError::SymbolNotFound(_))));
}

// ============================================================================
// DeleteComponentCommand Tests
// ============================================================================

#[test]
fn test_delete_component() {
    let mut schematic = Schematic::new();

    // Place component
    let symbol = create_resistor_symbol();
    let place_cmd = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: Some("R1".to_string()),
        value: None,
    };
    place_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.symbols.len(), 1);
    assert_eq!(schematic.lib_symbols.len(), 1);

    // Delete component
    let delete_cmd = DeleteComponentCommand {
        reference: "R1".to_string(),
    };
    delete_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.symbols.len(), 0);
    // lib_symbol should also be removed since nothing uses it
    assert_eq!(schematic.lib_symbols.len(), 0);
}

#[test]
fn test_delete_component_not_found() {
    let mut schematic = Schematic::new();

    let cmd = DeleteComponentCommand {
        reference: "R99".to_string(),
    };

    let result = cmd.execute(&mut schematic);
    assert!(matches!(result, Err(CommandError::SymbolNotFound(_))));
}

#[test]
fn test_delete_component_keeps_shared_lib_symbol() {
    let mut schematic = Schematic::new();
    let symbol = create_resistor_symbol();

    // Place two components using same lib_symbol
    let place_cmd1 = PlaceComponentCommand {
        symbol: symbol.clone(),
        lib_id: "Device:R".to_string(),
        position: Position::new(100.0, 50.0, 0.0),
        reference: Some("R1".to_string()),
        value: None,
    };
    place_cmd1.execute(&mut schematic).unwrap();

    let place_cmd2 = PlaceComponentCommand {
        symbol,
        lib_id: "Device:R".to_string(),
        position: Position::new(120.0, 50.0, 0.0),
        reference: Some("R2".to_string()),
        value: None,
    };
    place_cmd2.execute(&mut schematic).unwrap();

    assert_eq!(schematic.symbols.len(), 2);
    assert_eq!(schematic.lib_symbols.len(), 1);

    // Delete R1
    let delete_cmd = DeleteComponentCommand {
        reference: "R1".to_string(),
    };
    delete_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.symbols.len(), 1);
    // lib_symbol should be kept since R2 still uses it
    assert_eq!(schematic.lib_symbols.len(), 1);
}

// ============================================================================
// DeleteWireCommand Tests
// ============================================================================

#[test]
fn test_delete_wire() {
    let mut schematic = Schematic::new();

    // Add a wire
    let add_cmd = AddWireCommand {
        from: WireEndpoint::Point(Point::new(100.0, 50.0)),
        to: WireEndpoint::Point(Point::new(150.0, 50.0)),
        routing: RoutingMode::Direct,
    };
    add_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.wires.len(), 1);

    // Delete the wire
    let delete_cmd = DeleteWireCommand {
        point: Point::new(125.0, 50.0),
    };
    delete_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.wires.len(), 0);
}

#[test]
fn test_delete_wire_not_found() {
    let mut schematic = Schematic::new();

    // Add a wire
    let add_cmd = AddWireCommand {
        from: WireEndpoint::Point(Point::new(100.0, 50.0)),
        to: WireEndpoint::Point(Point::new(150.0, 50.0)),
        routing: RoutingMode::Direct,
    };
    add_cmd.execute(&mut schematic).unwrap();

    // Try to delete at wrong position
    let delete_cmd = DeleteWireCommand {
        point: Point::new(200.0, 200.0),
    };

    let result = delete_cmd.execute(&mut schematic);
    assert!(matches!(result, Err(CommandError::WireNotFound(_))));
    assert_eq!(schematic.wires.len(), 1); // Wire should still exist
}

// ============================================================================
// DeleteLabelCommand Tests
// ============================================================================

#[test]
fn test_delete_label_by_name() {
    let mut schematic = Schematic::new();

    // Add a label
    let add_cmd = AddLabelCommand {
        text: "NET1".to_string(),
        location: LabelLocation::Position(Position::new(100.0, 50.0, 0.0)),
        global: false,
        shape: LabelShape::Input,
    };
    add_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.labels.len(), 1);

    // Delete by name
    let delete_cmd = DeleteLabelCommand {
        name: "NET1".to_string(),
        position: None,
    };
    delete_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.labels.len(), 0);
}

#[test]
fn test_delete_label_by_position() {
    let mut schematic = Schematic::new();

    // Add two labels with same name at different positions
    let add_cmd1 = AddLabelCommand {
        text: "NET".to_string(),
        location: LabelLocation::Position(Position::new(100.0, 50.0, 0.0)),
        global: false,
        shape: LabelShape::Input,
    };
    add_cmd1.execute(&mut schematic).unwrap();

    let add_cmd2 = AddLabelCommand {
        text: "NET".to_string(),
        location: LabelLocation::Position(Position::new(150.0, 50.0, 0.0)),
        global: false,
        shape: LabelShape::Input,
    };
    add_cmd2.execute(&mut schematic).unwrap();

    assert_eq!(schematic.labels.len(), 2);

    // Get the snapped position of the second label
    let second_label_pos = schematic.labels[1].position;

    // Delete only the second one by position
    let delete_cmd = DeleteLabelCommand {
        name: "NET".to_string(),
        position: Some(Point::new(second_label_pos.x, second_label_pos.y)),
    };
    delete_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.labels.len(), 1);
}

#[test]
fn test_delete_label_not_found() {
    let mut schematic = Schematic::new();

    let cmd = DeleteLabelCommand {
        name: "NONEXISTENT".to_string(),
        position: None,
    };

    let result = cmd.execute(&mut schematic);
    assert!(matches!(result, Err(CommandError::LabelNotFound { .. })));
}

#[test]
fn test_delete_global_label() {
    let mut schematic = Schematic::new();

    // Add global label
    let add_cmd = AddLabelCommand {
        text: "VCC".to_string(),
        location: LabelLocation::Position(Position::new(100.0, 50.0, 0.0)),
        global: true,
        shape: LabelShape::Input,
    };
    add_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.global_labels.len(), 1);

    // Delete it
    let delete_cmd = DeleteLabelCommand {
        name: "VCC".to_string(),
        position: None,
    };
    delete_cmd.execute(&mut schematic).unwrap();

    assert_eq!(schematic.global_labels.len(), 0);
}

// ============================================================================
// Helper Function Tests
// ============================================================================

#[test]
fn test_parse_pin_ref() {
    let (reference, pin) = parse_pin_ref("R1:1").unwrap();
    assert_eq!(reference, "R1");
    assert_eq!(pin, "1");

    let (reference, pin) = parse_pin_ref("U1:VCC").unwrap();
    assert_eq!(reference, "U1");
    assert_eq!(pin, "VCC");

    let result = parse_pin_ref("invalid");
    assert!(matches!(result, Err(CommandError::InvalidPinRef(_))));
}

#[test]
fn test_parse_routing_mode() {
    assert_eq!(parse_routing_mode("direct").unwrap(), RoutingMode::Direct);
    assert_eq!(
        parse_routing_mode("orthogonal").unwrap(),
        RoutingMode::Orthogonal
    );
    assert_eq!(
        parse_routing_mode("orthogonal-vh").unwrap(),
        RoutingMode::OrthogonalVH
    );
    assert!(matches!(
        parse_routing_mode("invalid"),
        Err(CommandError::InvalidRoutingMode(_))
    ));
}

#[test]
fn test_parse_label_shape() {
    assert_eq!(parse_label_shape("input").unwrap(), LabelShape::Input);
    assert_eq!(parse_label_shape("output").unwrap(), LabelShape::Output);
    assert_eq!(
        parse_label_shape("bidirectional").unwrap(),
        LabelShape::Bidirectional
    );
    assert_eq!(
        parse_label_shape("tri_state").unwrap(),
        LabelShape::TriState
    );
    assert_eq!(parse_label_shape("passive").unwrap(), LabelShape::Passive);
    assert!(matches!(
        parse_label_shape("invalid"),
        Err(CommandError::InvalidLabelShape(_))
    ));
}

