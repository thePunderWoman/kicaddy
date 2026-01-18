//! DeleteLabelCommand - delete a label from a schematic

use crate::commands::{Command, CommandError};
use crate::common::Point;
use crate::schematic::Schematic;

/// Command to delete a label from a schematic
pub struct DeleteLabelCommand {
    /// Label text to delete
    pub name: String,
    /// Optional position to disambiguate multiple labels with the same name
    pub position: Option<Point>,
}

impl Command for DeleteLabelCommand {
    type Output = ();

    fn execute(self, schematic: &mut Schematic) -> Result<Self::Output, CommandError> {
        if !schematic.delete_label(&self.name, self.position) {
            return Err(CommandError::LabelNotFound {
                name: self.name,
                position: self.position,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Position;
    use crate::schematic::{GlobalLabel, Label, LabelShape};

    #[test]
    fn test_delete_local_label_by_name() {
        let mut schematic = Schematic::new();

        schematic.labels.push(Label {
            text: "NET1".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-1".to_string(),
        });

        let cmd = DeleteLabelCommand {
            name: "NET1".to_string(),
            position: None,
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.labels.len(), 0);
    }

    #[test]
    fn test_delete_global_label() {
        let mut schematic = Schematic::new();

        schematic.global_labels.push(GlobalLabel {
            text: "VCC".to_string(),
            shape: LabelShape::Input,
            position: Position::new(100.0, 50.0, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-1".to_string(),
            properties: vec![],
        });

        let cmd = DeleteLabelCommand {
            name: "VCC".to_string(),
            position: None,
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.global_labels.len(), 0);
    }

    #[test]
    fn test_delete_label_by_position() {
        let mut schematic = Schematic::new();

        // Add two labels with the same name at different positions
        schematic.labels.push(Label {
            text: "NET".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-1".to_string(),
        });
        schematic.labels.push(Label {
            text: "NET".to_string(),
            position: Position::new(150.0, 50.0, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-2".to_string(),
        });

        // Delete only the one at position (150, 50)
        let cmd = DeleteLabelCommand {
            name: "NET".to_string(),
            position: Some(Point::new(150.0, 50.0)),
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.labels.len(), 1);
        assert_eq!(schematic.labels[0].uuid, "label-1"); // First label should remain
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
    fn test_delete_label_wrong_position() {
        let mut schematic = Schematic::new();

        schematic.labels.push(Label {
            text: "NET1".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-1".to_string(),
        });

        // Try to delete at wrong position
        let cmd = DeleteLabelCommand {
            name: "NET1".to_string(),
            position: Some(Point::new(200.0, 200.0)),
        };

        let result = cmd.execute(&mut schematic);
        assert!(matches!(result, Err(CommandError::LabelNotFound { .. })));
        assert_eq!(schematic.labels.len(), 1); // Label should still exist
    }

    #[test]
    fn test_delete_first_matching_label_when_no_position() {
        let mut schematic = Schematic::new();

        // Add multiple labels with the same name
        schematic.labels.push(Label {
            text: "NET".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-1".to_string(),
        });
        schematic.labels.push(Label {
            text: "NET".to_string(),
            position: Position::new(150.0, 50.0, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-2".to_string(),
        });

        // Delete without specifying position - should delete the first match
        let cmd = DeleteLabelCommand {
            name: "NET".to_string(),
            position: None,
        };

        cmd.execute(&mut schematic).unwrap();

        assert_eq!(schematic.labels.len(), 1);
        // The first one should be deleted, leaving the second
        assert_eq!(schematic.labels[0].uuid, "label-2");
    }
}
