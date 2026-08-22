//! Syncing kicaddy's compiled output against a sibling `.kicad_pro` project file.
//!
//! KiCad's `.kicad_pro` caches a `"sheets"` list (uuid + name, one per sheet including the
//! root) and a `"schematic"."top_level_sheets"` entry (the same shape, root only) rather than
//! deriving them live from the `.kicad_sch` files on load. `compile()` generates fresh sheet
//! UUIDs on every run, so immediately after any `kicaddy compile`, a pre-existing `.kicad_pro`'s
//! cached list is stale — that mismatch is what makes the KiCad GUI show "An error was found
//! when loading the schematic that has been automatically fixed" on first open: it silently
//! reconciles its in-memory model against the real file UUIDs and flags the project dirty.

use std::path::Path;

use serde_json::Value;

/// One sheet's cached identity in `.kicad_pro`: its uuid and display name.
pub struct ProjectSheet {
    pub uuid: String,
    pub name: String,
}

/// Sync `root_output_path`'s sibling `.kicad_pro` (same file stem, `.kicad_pro` extension) so
/// its cached `"sheets"` list and `"schematic"."top_level_sheets"` entry match the sheet UUIDs
/// just written to disk. `root` must be the compiled root schematic's own identity (its
/// `hierarchy_root_uuid` and `project_name`); `children` is every other sheet, in any order.
///
/// A no-op when no `.kicad_pro` exists next to `root_output_path` — kicaddy doesn't create
/// KiCad projects, only schematics, so an unsynced project is expected in that case, not an
/// error. Every other field in the file is left untouched (round-tripped as-is).
pub fn sync_kicad_pro_sheets(
    root_output_path: &Path,
    root: &ProjectSheet,
    children: &[ProjectSheet],
) -> std::io::Result<()> {
    let pro_path = root_output_path.with_extension("kicad_pro");
    if !pro_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&pro_path)?;
    let mut doc: Value = serde_json::from_str(&content)?;

    let sheets_list: Vec<Value> = std::iter::once(root)
        .chain(children.iter())
        .map(|s| Value::Array(vec![Value::String(s.uuid.clone()), Value::String(s.name.clone())]))
        .collect();
    doc["sheets"] = Value::Array(sheets_list);

    let root_filename = root_output_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or_default();
    doc["schematic"]["top_level_sheets"] = Value::Array(vec![serde_json::json!({
        "filename": root_filename,
        "name": root.name,
        "uuid": root.uuid,
    })]);

    let updated = serde_json::to_string_pretty(&doc)?;
    std::fs::write(&pro_path, updated)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_kicad_pro_sheets_updates_sheets_and_top_level_sheets() {
        let dir = std::env::temp_dir().join(format!("kicaddy_project_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let root_path = dir.join("myproject.kicad_sch");
        let pro_path = dir.join("myproject.kicad_pro");

        // A stale project file: uuids from a previous compile, plus fields kicaddy has no
        // business touching (confirms those round-trip untouched).
        std::fs::write(
            &pro_path,
            r#"{
                "sheets": [["stale-root-uuid", "myproject"], ["stale-child-uuid", "Old_Sheet"]],
                "schematic": {
                    "top_level_sheets": [{"filename": "myproject.kicad_sch", "name": "myproject", "uuid": "stale-root-uuid"}],
                    "annotate_start_num": 0
                },
                "board": {"design_settings": {}}
            }"#,
        )
        .unwrap();

        let root = ProjectSheet {
            uuid: "fresh-root-uuid".to_string(),
            name: "myproject".to_string(),
        };
        let children = vec![
            ProjectSheet {
                uuid: "fresh-child-uuid-1".to_string(),
                name: "Power".to_string(),
            },
            ProjectSheet {
                uuid: "fresh-child-uuid-2".to_string(),
                name: "Logic".to_string(),
            },
        ];

        sync_kicad_pro_sheets(&root_path, &root, &children).unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&pro_path).unwrap()).unwrap();

        assert_eq!(
            updated["sheets"],
            serde_json::json!([
                ["fresh-root-uuid", "myproject"],
                ["fresh-child-uuid-1", "Power"],
                ["fresh-child-uuid-2", "Logic"],
            ])
        );
        assert_eq!(
            updated["schematic"]["top_level_sheets"],
            serde_json::json!([{
                "filename": "myproject.kicad_sch",
                "name": "myproject",
                "uuid": "fresh-root-uuid",
            }])
        );
        // Untouched fields survive the round-trip.
        assert_eq!(updated["schematic"]["annotate_start_num"], 0);
        assert!(updated["board"]["design_settings"].is_object());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_sync_kicad_pro_sheets_is_noop_without_a_kicad_pro() {
        let dir = std::env::temp_dir().join(format!("kicaddy_project_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let root_path = dir.join("myproject.kicad_sch");

        let root = ProjectSheet {
            uuid: "fresh-root-uuid".to_string(),
            name: "myproject".to_string(),
        };

        // No myproject.kicad_pro exists in `dir` — must not error, and must not create one.
        sync_kicad_pro_sheets(&root_path, &root, &[]).unwrap();
        assert!(!dir.join("myproject.kicad_pro").exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}
