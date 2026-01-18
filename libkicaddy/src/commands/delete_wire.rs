//! DeleteWireCommand - delete a wire at or near a point

use crate::commands::{Command, CommandError};
use crate::common::Point;
use crate::schematic::Schematic;

/// Command to delete a wire from a schematic
pub struct DeleteWireCommand {
    /// Point on or near the wire to delete
    pub point: Point,
}

impl Command for DeleteWireCommand {
    type Output = ();

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        if !schematic.delete_wire_at(self.point) {
            return Err(CommandError::WireNotFound(self.point));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Point, Stroke};
    use crate::schematic::Wire;

    #[test]
    fn test_delete_wire_basic() {
        let mut schematic = Schematic::new();

        // Add a wire
        schematic.wires.push(Wire {
            points: vec![Point::new(100.0, 50.0), Point::new(150.0, 50.0)],
            stroke: Stroke::default(),
            uuid: "test-wire".to_string(),
        });

        assert_eq!(schematic.wires.len(), 1);

        let cmd = DeleteWireCommand {
            point: Point::new(125.0, 50.0), // Point on the wire
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.wires.len(), 0);
    }

    #[test]
    fn test_delete_wire_near_point() {
        let mut schematic = Schematic::new();

        // Add a wire
        schematic.wires.push(Wire {
            points: vec![Point::new(100.0, 50.0), Point::new(150.0, 50.0)],
            stroke: Stroke::default(),
            uuid: "test-wire".to_string(),
        });

        // Use a point near the wire (within tolerance)
        let cmd = DeleteWireCommand {
            point: Point::new(125.0, 50.5),
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.wires.len(), 0);
    }

    #[test]
    fn test_delete_wire_not_found() {
        let mut schematic = Schematic::new();

        // Add a wire
        schematic.wires.push(Wire {
            points: vec![Point::new(100.0, 50.0), Point::new(150.0, 50.0)],
            stroke: Stroke::default(),
            uuid: "test-wire".to_string(),
        });

        // Try to delete at a point far from the wire
        let cmd = DeleteWireCommand {
            point: Point::new(200.0, 200.0),
        };

        let result = cmd.execute(&mut schematic);
        assert!(matches!(result, Err(CommandError::WireNotFound(_))));
        assert_eq!(schematic.wires.len(), 1); // Wire should still exist
    }

    #[test]
    fn test_delete_wire_at_endpoint() {
        let mut schematic = Schematic::new();

        schematic.wires.push(Wire {
            points: vec![Point::new(100.0, 50.0), Point::new(150.0, 50.0)],
            stroke: Stroke::default(),
            uuid: "test-wire".to_string(),
        });

        // Delete at the start endpoint
        let cmd = DeleteWireCommand {
            point: Point::new(100.0, 50.0),
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.wires.len(), 0);
    }

    #[test]
    fn test_delete_wire_multi_segment() {
        let mut schematic = Schematic::new();

        // Add a wire with multiple segments (orthogonal routing)
        schematic.wires.push(Wire {
            points: vec![
                Point::new(100.0, 50.0),
                Point::new(150.0, 50.0),
                Point::new(150.0, 80.0),
            ],
            stroke: Stroke::default(),
            uuid: "test-wire".to_string(),
        });

        // Delete at a point on the second segment
        let cmd = DeleteWireCommand {
            point: Point::new(150.0, 65.0),
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.wires.len(), 0);
    }

    #[test]
    fn test_delete_correct_wire_when_multiple() {
        let mut schematic = Schematic::new();

        // Add two wires
        schematic.wires.push(Wire {
            points: vec![Point::new(100.0, 50.0), Point::new(150.0, 50.0)],
            stroke: Stroke::default(),
            uuid: "wire-1".to_string(),
        });
        schematic.wires.push(Wire {
            points: vec![Point::new(100.0, 100.0), Point::new(150.0, 100.0)],
            stroke: Stroke::default(),
            uuid: "wire-2".to_string(),
        });

        assert_eq!(schematic.wires.len(), 2);

        // Delete the first wire
        let cmd = DeleteWireCommand {
            point: Point::new(125.0, 50.0),
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.wires.len(), 1);
        assert_eq!(schematic.wires[0].uuid, "wire-2"); // Only wire-2 should remain
    }
}
