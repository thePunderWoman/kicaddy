//! AddLabelCommand - add a label to a schematic

use crate::commands::{angle_from_pin_direction, Command, CommandError};
use crate::common::Position;
use crate::schematic::{LabelShape, Schematic};

/// Specifies the location of a label
#[derive(Debug, Clone)]
pub enum LabelLocation {
    /// Place at a pin (auto-detects angle from pin direction)
    Pin { reference: String, pin: String },
    /// Place at explicit position with angle
    Position(Position),
}

impl LabelLocation {
    /// Create a pin-based location
    pub fn pin(reference: impl Into<String>, pin: impl Into<String>) -> Self {
        LabelLocation::Pin {
            reference: reference.into(),
            pin: pin.into(),
        }
    }

    /// Create an explicit position
    pub fn position(x: f64, y: f64, angle: f64) -> Self {
        LabelLocation::Position(Position::new(x, y, angle))
    }
}

/// Command to add a label to a schematic
pub struct AddLabelCommand {
    /// The label text
    pub text: String,
    /// Where to place the label
    pub location: LabelLocation,
    /// Whether this is a global label (true) or local label (false)
    pub global: bool,
    /// Shape for global labels (ignored if global=false)
    pub shape: LabelShape,
}

/// Result of adding a label
pub struct AddLabelResult {
    /// The label text
    pub text: String,
    /// The resolved position
    pub position: Position,
    /// Whether the label is global
    pub global: bool,
}

impl Command for AddLabelCommand {
    type Output = AddLabelResult;

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        // Resolve position and angle
        let position = match &self.location {
            LabelLocation::Pin { reference, pin } => {
                let symbol = schematic
                    .find_symbol_by_reference(reference)
                    .ok_or_else(|| CommandError::SymbolNotFound(reference.to_string()))?;

                let (point, pin_angle) =
                    schematic
                        .get_pin_position(symbol, pin)
                        .ok_or_else(|| CommandError::PinNotFound {
                            reference: reference.to_string(),
                            pin: pin.to_string(),
                        })?;

                // Auto-detect angle based on pin direction
                let label_angle = angle_from_pin_direction(pin_angle);
                Position::new(point.x, point.y, label_angle)
            }
            LabelLocation::Position(pos) => *pos,
        };

        // Add the label
        if self.global {
            schematic.add_global_label(&self.text, position, self.shape);
        } else {
            schematic.add_label(&self.text, position);
        }

        Ok(AddLabelResult {
            text: self.text,
            position,
            global: self.global,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Position as CommonPosition, Property};
    use crate::schematic::SymbolInstance;
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
                pins: vec![Pin {
                    electrical_type: PinElectricalType::Passive,
                    graphic_style: PinGraphicStyle::Line,
                    position: CommonPosition::new(0.0, 2.54, 270.0),
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
                }],
            }],
            embedded_fonts: None,
        };

        schematic.lib_symbols.push(lib_symbol);

        let symbol_instance = SymbolInstance {
            lib_id: "Device:R".to_string(),
            position: CommonPosition::new(100.0, 50.0, 0.0),
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
    fn test_add_local_label_at_position() {
        let mut schematic = Schematic::new();

        let cmd = AddLabelCommand {
            text: "NET1".to_string(),
            location: LabelLocation::position(100.0, 50.0, 0.0),
            global: false,
            shape: LabelShape::Input,
        };

        let result = cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.labels.len(), 1);
        assert_eq!(result.text, "NET1");
        assert!(!result.global);
    }

    #[test]
    fn test_add_global_label_at_position() {
        let mut schematic = Schematic::new();

        let cmd = AddLabelCommand {
            text: "VCC".to_string(),
            location: LabelLocation::position(100.0, 50.0, 0.0),
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
        let mut schematic = create_test_schematic_with_resistor();

        let cmd = AddLabelCommand {
            text: "SIGNAL".to_string(),
            location: LabelLocation::pin("R1", "1"),
            global: false,
            shape: LabelShape::Input,
        };

        let result = cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.labels.len(), 1);
        assert_eq!(result.text, "SIGNAL");
        // Position should be at pin location
        assert!((result.position.x - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_add_label_symbol_not_found() {
        let mut schematic = Schematic::new();

        let cmd = AddLabelCommand {
            text: "NET".to_string(),
            location: LabelLocation::pin("R99", "1"),
            global: false,
            shape: LabelShape::Input,
        };

        let result = cmd.execute(&mut schematic);
        assert!(matches!(result, Err(CommandError::SymbolNotFound(_))));
    }

    #[test]
    fn test_add_label_with_different_shapes() {
        let mut schematic = Schematic::new();

        for shape in [
            LabelShape::Input,
            LabelShape::Output,
            LabelShape::Bidirectional,
            LabelShape::TriState,
            LabelShape::Passive,
        ] {
            let cmd = AddLabelCommand {
                text: format!("NET_{:?}", shape),
                location: LabelLocation::position(100.0, 50.0, 0.0),
                global: true,
                shape,
            };

            cmd.execute(&mut schematic).unwrap();
        }

        assert_eq!(schematic.global_labels.len(), 5);
    }
}
