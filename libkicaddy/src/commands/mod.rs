//! Command pattern for schematic operations
//!
//! Each operation is a Command struct with an `execute(&mut Schematic)` method.
//! This separates business logic from the CLI for better testability and reuse.

pub mod add_label;
pub mod add_wire;
pub mod connect;
pub mod delete_component;
pub mod delete_label;
pub mod delete_wire;
pub mod disconnect;
pub mod place_component;

pub use add_label::{AddLabelCommand, LabelLocation};
pub use add_wire::{AddWireCommand, WireEndpoint};
pub use connect::{ConnectCommand, ConnectOutput};
pub use delete_component::DeleteComponentCommand;
pub use delete_label::DeleteLabelCommand;
pub use delete_wire::DeleteWireCommand;
pub use disconnect::{DisconnectCommand, DisconnectOutput};
pub use place_component::PlaceComponentCommand;

use crate::common::Point;
use crate::schematic::{LabelShape, RoutingMode, Schematic};

/// The Command trait for schematic operations
pub trait Command {
    /// The output type returned on successful execution
    type Output;

    /// Execute the command on the given schematic
    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError>;
}

/// Errors that can occur during command execution
#[derive(Debug, Clone, PartialEq)]
pub enum CommandError {
    /// Symbol/component not found by reference
    SymbolNotFound(String),
    /// Pin not found on symbol
    PinNotFound { reference: String, pin: String },
    /// Wire not found at position
    WireNotFound(Point),
    /// Label not found
    LabelNotFound { name: String, position: Option<Point> },
    /// Invalid pin reference format
    InvalidPinRef(String),
    /// Invalid routing mode
    InvalidRoutingMode(String),
    /// Invalid label shape
    InvalidLabelShape(String),
    /// Other errors
    Other(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::SymbolNotFound(r) => write!(f, "Symbol '{}' not found", r),
            CommandError::PinNotFound { reference, pin } => {
                write!(f, "Pin '{}' not found on '{}'", pin, reference)
            }
            CommandError::WireNotFound(p) => write!(f, "No wire found at ({}, {})", p.x, p.y),
            CommandError::LabelNotFound { name, position } => {
                if let Some(pos) = position {
                    write!(f, "Label '{}' not found at ({}, {})", name, pos.x, pos.y)
                } else {
                    write!(f, "Label '{}' not found", name)
                }
            }
            CommandError::InvalidPinRef(s) => {
                write!(f, "Invalid pin reference '{}', expected 'REF:PIN' format", s)
            }
            CommandError::InvalidRoutingMode(s) => {
                write!(
                    f,
                    "Invalid routing mode '{}'. Use: direct, orthogonal, or orthogonal-vh",
                    s
                )
            }
            CommandError::InvalidLabelShape(s) => {
                write!(
                    f,
                    "Invalid label shape '{}'. Use: input, output, bidirectional, tri_state, or passive",
                    s
                )
            }
            CommandError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for CommandError {}

/// KiCAD grid constant (50 mils = 1.27mm)
pub const GRID: f64 = 1.27;

/// Snap a coordinate to the KiCAD grid
pub fn snap_to_grid(value: f64) -> f64 {
    (value / GRID).round() * GRID
}

/// Snap a point to the KiCAD grid
pub fn snap_point_to_grid(point: Point) -> Point {
    Point::new(snap_to_grid(point.x), snap_to_grid(point.y))
}

/// Parse "REF:PIN" format into (reference, pin)
pub fn parse_pin_ref(s: &str) -> Result<(String, String), CommandError> {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(CommandError::InvalidPinRef(s.to_string()));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse routing mode from string
pub fn parse_routing_mode(s: &str) -> Result<RoutingMode, CommandError> {
    match s {
        "direct" => Ok(RoutingMode::Direct),
        "orthogonal" => Ok(RoutingMode::Orthogonal),
        "orthogonal-vh" => Ok(RoutingMode::OrthogonalVH),
        _ => Err(CommandError::InvalidRoutingMode(s.to_string())),
    }
}

/// Parse label shape from string
pub fn parse_label_shape(s: &str) -> Result<LabelShape, CommandError> {
    match s {
        "input" => Ok(LabelShape::Input),
        "output" => Ok(LabelShape::Output),
        "bidirectional" => Ok(LabelShape::Bidirectional),
        "tri_state" => Ok(LabelShape::TriState),
        "passive" => Ok(LabelShape::Passive),
        _ => Err(CommandError::InvalidLabelShape(s.to_string())),
    }
}

/// Resolve a WireEndpoint to a Point
pub fn resolve_endpoint(
    schematic: &Schematic,
    reference: &str,
    pin: &str,
) -> Result<Point, CommandError> {
    let symbol = schematic
        .find_symbol_by_reference(reference)
        .ok_or_else(|| CommandError::SymbolNotFound(reference.to_string()))?;

    schematic
        .get_pin_position(symbol, pin)
        .map(|(point, _)| point)
        .ok_or_else(|| CommandError::PinNotFound {
            reference: reference.to_string(),
            pin: pin.to_string(),
        })
}

/// Auto-detect label angle from pin direction
/// The label should face the same direction as the pin
pub fn angle_from_pin_direction(pin_angle: f64) -> f64 {
    match (pin_angle as i32) % 360 {
        0 => 0.0,     // Pin points right, label faces right
        90 => 90.0,   // Pin points up, label faces up
        180 => 180.0, // Pin points left, label faces left
        270 => 270.0, // Pin points down, label faces down
        _ => pin_angle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pin_ref_valid() {
        let (reference, pin) = parse_pin_ref("R1:1").unwrap();
        assert_eq!(reference, "R1");
        assert_eq!(pin, "1");
    }

    #[test]
    fn test_parse_pin_ref_with_complex_names() {
        let (reference, pin) = parse_pin_ref("U1:VCC").unwrap();
        assert_eq!(reference, "U1");
        assert_eq!(pin, "VCC");
    }

    #[test]
    fn test_parse_pin_ref_invalid() {
        let result = parse_pin_ref("R1-1");
        assert!(matches!(result, Err(CommandError::InvalidPinRef(_))));
    }

    #[test]
    fn test_snap_to_grid() {
        assert!((snap_to_grid(100.0) - 100.33).abs() < 0.01);
        assert!((snap_to_grid(1.27) - 1.27).abs() < 0.001);
        assert!((snap_to_grid(0.0) - 0.0).abs() < 0.001);
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
        assert!(parse_routing_mode("invalid").is_err());
    }

    #[test]
    fn test_parse_label_shape() {
        assert_eq!(parse_label_shape("input").unwrap(), LabelShape::Input);
        assert_eq!(parse_label_shape("output").unwrap(), LabelShape::Output);
        assert_eq!(
            parse_label_shape("bidirectional").unwrap(),
            LabelShape::Bidirectional
        );
        assert!(parse_label_shape("invalid").is_err());
    }

    #[test]
    fn test_angle_from_pin_direction() {
        assert_eq!(angle_from_pin_direction(0.0), 0.0);
        assert_eq!(angle_from_pin_direction(90.0), 90.0);
        assert_eq!(angle_from_pin_direction(180.0), 180.0);
        assert_eq!(angle_from_pin_direction(270.0), 270.0);
    }
}
