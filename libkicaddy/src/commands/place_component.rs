//! PlaceComponentCommand - place a component from a symbol library

use crate::commands::{snap_to_grid, Command, CommandError};
use crate::common::Position;
use crate::schematic::Schematic;
use crate::symbol::Symbol;

/// Command to place a component in a schematic
pub struct PlaceComponentCommand {
    /// The symbol definition from the library
    pub symbol: Symbol,
    /// Library ID in "Library:Symbol" format (e.g., "Device:R")
    pub lib_id: String,
    /// Position to place the symbol (will be snapped to grid)
    pub position: Position,
    /// Optional reference designator (e.g., "R1"). If None, uses symbol's default
    pub reference: Option<String>,
    /// Optional component value. If None, uses symbol name
    pub value: Option<String>,
}

/// Result of placing a component
pub struct PlaceComponentResult {
    /// The reference designator used
    pub reference: String,
    /// The snapped X position
    pub x: f64,
    /// The snapped Y position
    pub y: f64,
    /// Whether the position was snapped (different from requested)
    pub was_snapped: bool,
}

impl Command for PlaceComponentCommand {
    type Output = PlaceComponentResult;

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        // Snap position to grid
        let snapped_x = snap_to_grid(self.position.x);
        let snapped_y = snap_to_grid(self.position.y);
        let was_snapped = snapped_x != self.position.x || snapped_y != self.position.y;

        let snapped_position = Position::new(snapped_x, snapped_y, self.position.angle);

        // Determine the reference that will be used
        let reference = self
            .reference
            .clone()
            .or_else(|| self.symbol.reference().map(|s| format!("{}?", s)))
            .unwrap_or_else(|| "U?".to_string());

        // Add the symbol to the schematic
        schematic.add_symbol(
            &self.symbol,
            &self.lib_id,
            snapped_position,
            self.reference.as_deref(),
            self.value.as_deref(),
        );

        Ok(PlaceComponentResult {
            reference,
            x: snapped_x,
            y: snapped_y,
            was_snapped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{Symbol, SymbolUnit};

    fn create_test_symbol() -> Symbol {
        Symbol {
            name: "R".to_string(),
            pin_numbers_hide: false,
            pin_names_offset: 0.0,
            pin_names_hide: false,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            properties: vec![crate::common::Property {
                name: "Reference".to_string(),
                value: "R".to_string(),
                position: None,
                effects: None,
            }],
            units: vec![SymbolUnit {
                name: "R_0_1".to_string(),
                graphics: vec![],
                pins: vec![],
            }],
            embedded_fonts: None,
        }
    }

    #[test]
    fn test_place_component_basic() {
        let mut schematic = Schematic::new();
        let symbol = create_test_symbol();

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
    }

    #[test]
    fn test_place_component_snaps_to_grid() {
        let mut schematic = Schematic::new();
        let symbol = create_test_symbol();

        let cmd = PlaceComponentCommand {
            symbol,
            lib_id: "Device:R".to_string(),
            position: Position::new(100.5, 50.3, 0.0), // Not on grid
            reference: Some("R1".to_string()),
            value: None,
        };

        let result = cmd.execute(&mut schematic).unwrap();

        // Should be snapped to grid
        assert!(result.was_snapped);
        assert!((result.x - 100.33).abs() < 0.01); // Snapped to 1.27 grid
        assert!((result.y - 50.8).abs() < 0.01);
    }

    #[test]
    fn test_place_component_default_reference() {
        let mut schematic = Schematic::new();
        let symbol = create_test_symbol();

        let cmd = PlaceComponentCommand {
            symbol,
            lib_id: "Device:R".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            reference: None, // Use default
            value: None,
        };

        let result = cmd.execute(&mut schematic).unwrap();

        // Should use symbol's reference property with "?"
        assert_eq!(result.reference, "R?");
    }

    #[test]
    fn test_place_multiple_components_reuses_lib_symbol() {
        let mut schematic = Schematic::new();
        let symbol = create_test_symbol();

        // Place first component
        let cmd1 = PlaceComponentCommand {
            symbol: symbol.clone(),
            lib_id: "Device:R".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            reference: Some("R1".to_string()),
            value: None,
        };
        cmd1.execute(&mut schematic).unwrap();

        // Place second component with same lib_id
        let cmd2 = PlaceComponentCommand {
            symbol,
            lib_id: "Device:R".to_string(),
            position: Position::new(120.0, 50.0, 0.0),
            reference: Some("R2".to_string()),
            value: None,
        };
        cmd2.execute(&mut schematic).unwrap();

        // Should have 2 symbol instances but only 1 lib_symbol
        assert_eq!(schematic.symbols.len(), 2);
        assert_eq!(schematic.lib_symbols.len(), 1);
    }
}
