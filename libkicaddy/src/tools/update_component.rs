//! Update component tool - Modify component properties

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{load_schematic, save_schematic, Tool, ToolError};
use crate::commands::snap_to_grid;
use crate::schematic::Mirror;

/// Input for update_component tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateComponentInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Reference designator of component to update (e.g., "R1")
    pub reference: String,

    /// New X position (optional, snapped to grid)
    #[serde(default)]
    pub x: Option<f64>,

    /// New Y position (optional, snapped to grid)
    #[serde(default)]
    pub y: Option<f64>,

    /// New rotation angle in degrees (optional)
    #[serde(default)]
    pub angle: Option<f64>,

    /// New reference designator (optional, for renaming)
    #[serde(default)]
    pub new_reference: Option<String>,

    /// New value (optional)
    #[serde(default)]
    pub value: Option<String>,

    /// Mirror setting: "x", "y", or "none" (optional)
    #[serde(default)]
    pub mirror: Option<String>,
}

/// Output of update_component tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateComponentOutput {
    /// Reference of updated component
    pub reference: String,
    /// Final position X
    pub x: f64,
    /// Final position Y
    pub y: f64,
    /// Final angle
    pub angle: f64,
    /// Final value
    pub value: String,
    /// Whether position was snapped to grid
    pub was_snapped: bool,
    /// List of changes made
    pub changes: Vec<String>,
}

/// Update component tool implementation
pub struct UpdateComponentTool;

impl Tool for UpdateComponentTool {
    const NAME: &'static str = "update_component";
    const DESCRIPTION: &'static str =
        "Update component properties (position, angle, reference, value, mirror)";

    type Input = UpdateComponentInput;
    type Output = UpdateComponentOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut schematic = load_schematic(&input.schematic)?;

        // Find the symbol by reference
        let symbol_idx = schematic
            .symbols
            .iter()
            .position(|s| {
                s.properties
                    .iter()
                    .any(|p| p.name == "Reference" && p.value == input.reference)
            })
            .ok_or_else(|| ToolError::ComponentNotFound(input.reference.clone()))?;

        let mut changes: Vec<String> = Vec::new();
        let mut was_snapped = false;

        // Get current values for output
        let symbol = &mut schematic.symbols[symbol_idx];

        // Update position
        if let Some(new_x) = input.x {
            let snapped_x = snap_to_grid(new_x);
            if (snapped_x - new_x).abs() > 0.001 {
                was_snapped = true;
            }
            if (symbol.position.x - snapped_x).abs() > 0.001 {
                changes.push(format!("x: {} -> {}", symbol.position.x, snapped_x));
                symbol.position.x = snapped_x;
            }
        }

        if let Some(new_y) = input.y {
            let snapped_y = snap_to_grid(new_y);
            if (snapped_y - new_y).abs() > 0.001 {
                was_snapped = true;
            }
            if (symbol.position.y - snapped_y).abs() > 0.001 {
                changes.push(format!("y: {} -> {}", symbol.position.y, snapped_y));
                symbol.position.y = snapped_y;
            }
        }

        // Update angle
        if let Some(new_angle) = input.angle {
            // Normalize angle to 0-360
            let normalized_angle = ((new_angle % 360.0) + 360.0) % 360.0;
            if (symbol.position.angle - normalized_angle).abs() > 0.001 {
                changes.push(format!(
                    "angle: {} -> {}",
                    symbol.position.angle, normalized_angle
                ));
                symbol.position.angle = normalized_angle;
            }
        }

        // Update mirror
        if let Some(mirror_str) = &input.mirror {
            let new_mirror = match mirror_str.to_lowercase().as_str() {
                "x" => Some(Mirror::X),
                "y" => Some(Mirror::Y),
                "none" | "" => None,
                _ => {
                    return Err(ToolError::InvalidInput(format!(
                        "Invalid mirror value '{}'. Use 'x', 'y', or 'none'",
                        mirror_str
                    )))
                }
            };
            if symbol.mirror != new_mirror {
                changes.push(format!(
                    "mirror: {:?} -> {:?}",
                    symbol.mirror.map(|m| m.as_str()),
                    new_mirror.map(|m| m.as_str())
                ));
                symbol.mirror = new_mirror;
            }
        }

        // Update reference (rename)
        let final_reference = if let Some(new_ref) = &input.new_reference {
            if new_ref != &input.reference {
                // Check if new reference already exists
                let exists = schematic.symbols.iter().any(|s| {
                    s.properties
                        .iter()
                        .any(|p| p.name == "Reference" && p.value == *new_ref)
                });
                if exists {
                    return Err(ToolError::InvalidInput(format!(
                        "Reference '{}' already exists in schematic",
                        new_ref
                    )));
                }

                // Update reference property
                let symbol = &mut schematic.symbols[symbol_idx];
                if let Some(ref_prop) = symbol.properties.iter_mut().find(|p| p.name == "Reference")
                {
                    changes.push(format!("reference: {} -> {}", ref_prop.value, new_ref));
                    ref_prop.value = new_ref.clone();
                }

                // Update reference in instances
                for instance in &mut symbol.instances {
                    for path in &mut instance.paths {
                        if path.reference == input.reference {
                            path.reference = new_ref.clone();
                        }
                    }
                }

                new_ref.clone()
            } else {
                input.reference.clone()
            }
        } else {
            input.reference.clone()
        };

        // Update value
        let symbol = &mut schematic.symbols[symbol_idx];
        if let Some(new_value) = &input.value {
            if let Some(val_prop) = symbol.properties.iter_mut().find(|p| p.name == "Value") {
                if &val_prop.value != new_value {
                    changes.push(format!("value: {} -> {}", val_prop.value, new_value));
                    val_prop.value = new_value.clone();
                }
            }
        }

        // Note: Property positions are absolute in KiCAD and are typically
        // auto-placed by KiCAD when components move. We leave them as-is
        // and let fields_autoplaced handle repositioning when the schematic
        // is opened in KiCAD.

        // Get final values
        let symbol = &schematic.symbols[symbol_idx];
        let final_value = symbol
            .properties
            .iter()
            .find(|p| p.name == "Value")
            .map(|p| p.value.clone())
            .unwrap_or_default();

        // Save schematic if changes were made
        if !changes.is_empty() {
            save_schematic(&schematic, &input.schematic)?;
        }

        Ok(UpdateComponentOutput {
            reference: final_reference,
            x: symbol.position.x,
            y: symbol.position.y,
            angle: symbol.position.angle,
            value: final_value,
            was_snapped,
            changes,
        })
    }
}
