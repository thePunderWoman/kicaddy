//! DeleteComponentCommand - delete a component and its connected wires/labels

use crate::commands::{Command, CommandError};
use crate::schematic::Schematic;

/// Command to delete a component from a schematic
///
/// This also removes:
/// - Connected wires (endpoints at pin positions)
/// - Labels at pin positions
/// - The lib_symbol if no other instances use it
pub struct DeleteComponentCommand {
    /// Reference designator of the component to delete (e.g., "R1")
    pub reference: String,
}

impl Command for DeleteComponentCommand {
    type Output = ();

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        if !schematic.delete_symbol(&self.reference) {
            return Err(CommandError::SymbolNotFound(self.reference));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Position, Property};
    use crate::schematic::{Label, SymbolInstance, Wire};
    use crate::symbol::{
        Pin, PinElectricalType, PinGraphicStyle, PinName, PinNumber, Symbol, SymbolUnit,
    };

    fn create_test_schematic_with_resistor() -> Schematic {
        let mut schematic = Schematic::new();

        let lib_symbol = Symbol {
            name: "Device:R".to_string(),
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
    fn test_delete_component_basic() {
        let mut schematic = create_test_schematic_with_resistor();
        assert_eq!(schematic.symbols.len(), 1);
        assert_eq!(schematic.lib_symbols.len(), 1);

        let cmd = DeleteComponentCommand {
            reference: "R1".to_string(),
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.symbols.len(), 0);
        // lib_symbol should also be removed since no other instances use it
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
    fn test_delete_component_cleans_up_wires() {
        let mut schematic = create_test_schematic_with_resistor();

        // Get pin positions
        let symbol = schematic.find_symbol_by_reference("R1").unwrap();
        let pin_positions = schematic.get_all_pin_positions(symbol);

        // Add wires at pin positions
        for (pin_pos, _) in &pin_positions {
            use crate::common::{Point, Stroke};
            schematic.wires.push(Wire {
                points: vec![Point::new(80.0, pin_pos.y), *pin_pos],
                stroke: Stroke::default(),
                uuid: uuid::Uuid::new_v4().to_string(),
            });
        }

        assert_eq!(schematic.wires.len(), 2);

        let cmd = DeleteComponentCommand {
            reference: "R1".to_string(),
        };

        cmd.execute(&mut schematic).unwrap();

        // Wires at pin positions should be deleted
        assert_eq!(schematic.wires.len(), 0);
    }

    #[test]
    fn test_delete_component_cleans_up_labels() {
        let mut schematic = create_test_schematic_with_resistor();

        // Get pin positions
        let symbol = schematic.find_symbol_by_reference("R1").unwrap();
        let pin_positions = schematic.get_all_pin_positions(symbol);

        // Add labels at pin positions
        for (pin_pos, _) in &pin_positions {
            schematic.labels.push(Label {
                text: "TEST".to_string(),
                position: Position::new(pin_pos.x, pin_pos.y, 0.0),
                fields_autoplaced: true,
                effects: None,
                uuid: uuid::Uuid::new_v4().to_string(),
            });
        }

        assert_eq!(schematic.labels.len(), 2);

        let cmd = DeleteComponentCommand {
            reference: "R1".to_string(),
        };

        cmd.execute(&mut schematic).unwrap();

        // Labels at pin positions should be deleted
        assert_eq!(schematic.labels.len(), 0);
    }

    #[test]
    fn test_delete_component_keeps_shared_lib_symbol() {
        let mut schematic = create_test_schematic_with_resistor();

        // Add a second instance using the same lib_symbol
        let symbol_instance2 = SymbolInstance {
            lib_id: "Device:R".to_string(),
            position: Position::new(120.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: true,
            uuid: "test-uuid-2".to_string(),
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
        schematic.symbols.push(symbol_instance2);

        assert_eq!(schematic.symbols.len(), 2);
        assert_eq!(schematic.lib_symbols.len(), 1);

        let cmd = DeleteComponentCommand {
            reference: "R1".to_string(),
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.symbols.len(), 1);
        // lib_symbol should be kept since R2 still uses it
        assert_eq!(schematic.lib_symbols.len(), 1);
    }
}
