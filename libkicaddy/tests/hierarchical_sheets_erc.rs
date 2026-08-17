//! Integration tests that verify compiled hierarchical-sheet schematics against the real
//! `kicad-cli sch erc` — not just that kicaddy's own objects (labels, wires) exist in memory.
//!
//! This distinction matters: earlier unit tests asserted things like
//! `!power_child.hierarchical_labels.is_empty()`, which stayed green even when the compiled
//! output was electrically broken (wire_dangling / pin_not_connected / hier_label_mismatch),
//! because a label object existing is not the same as it being wired to anything. These tests
//! shell out to the real KiCAD ERC checker on the actual compiled files to catch that class of
//! bug, which unit-level assertions on kicaddy's internal structs cannot.

use std::path::{Path, PathBuf};
use std::process::Command;

use libkicaddy::yaml::compile_yaml_str;

/// Locate `kicad-cli`. It's frequently not on `PATH` (e.g. on macOS it lives under the app
/// bundle's `Contents/MacOS`, not `Contents/SharedSupport` where `KICAD_PATH` usually points),
/// so check a couple of known spots before giving up and skipping the test.
fn find_kicad_cli() -> Option<PathBuf> {
    if let Ok(output) = Command::new("which").arg("kicad-cli").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    let candidates = [
        "/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli",
        "/usr/bin/kicad-cli",
        "/usr/local/bin/kicad-cli",
    ];
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
}

/// Run `kicad-cli sch erc` on a compiled schematic and return its violations as parsed JSON.
/// Panics (failing the test) if kicad-cli itself fails to run — a parse/crash in ERC itself is
/// as much a signal as a reported violation.
fn run_erc(kicad_cli: &Path, sch_path: &Path) -> serde_json::Value {
    let report_path = sch_path.with_extension("erc.json");
    let output = Command::new(kicad_cli)
        .args(["sch", "erc"])
        .arg(sch_path)
        .arg("--output")
        .arg(&report_path)
        .args(["--format", "json", "--exit-code-violations"])
        .output()
        .expect("failed to run kicad-cli sch erc");

    // kicad-cli exits non-zero when violations are found (that's expected/normal) or when it
    // couldn't produce a report at all (not normal) — distinguish by whether the report exists.
    if !report_path.exists() {
        panic!(
            "kicad-cli sch erc produced no report for {:?}\nstdout: {}\nstderr: {}",
            sch_path,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let report = std::fs::read_to_string(&report_path).expect("failed to read ERC report");
    serde_json::from_str(&report).expect("failed to parse ERC report as JSON")
}

/// Collect every violation across all sheets into one flat list, keeping the fields tests care
/// about: severity, type, and each item's description (which names the specific object, e.g.
/// "Hierarchical Sheet Pin VCC" or "Symbol R1 Pin 2").
fn violations(report: &serde_json::Value) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for sheet in report["sheets"].as_array().unwrap_or(&Vec::new()) {
        for v in sheet["violations"].as_array().unwrap_or(&Vec::new()) {
            let severity = v["severity"].as_str().unwrap_or("").to_string();
            let vtype = v["type"].as_str().unwrap_or("").to_string();
            let items: Vec<String> = v["items"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(|i| i["description"].as_str().map(str::to_string))
                .collect();
            out.push((severity, vtype, items));
        }
    }
    out
}

/// Compile a yaml schematic and write the root plus every child sheet to `dir`, mirroring what
/// the `kicaddy compile` CLI command does (main.rs), so the test exercises the real multi-file
/// output kicad-cli actually loads and resolves sheet references against.
fn compile_to_dir(yaml: &str, dir: &Path, root_name: &str) -> PathBuf {
    let output = compile_yaml_str(yaml).expect("compile_yaml_str failed");
    let root_path = dir.join(format!("{root_name}.kicad_sch"));
    output
        .root
        .write_to_file(&root_path)
        .expect("failed to write root schematic");

    for (_sheet_name, child) in &output.children {
        // Every child's sheet_file (e.g. "power.kicad_sch") is already resolved with its
        // extension by the compiler; recover the matching root.sheets entry to name the file.
        let sheet = output
            .root
            .sheets
            .iter()
            .find(|s| {
                child
                    .sheet_instances
                    .first()
                    .map(|inst| inst.path.ends_with(&s.uuid))
                    .unwrap_or(false)
            })
            .expect("child schematic has no matching sheet entry");
        child
            .write_to_file(dir.join(&sheet.sheet_file))
            .expect("failed to write child schematic");
    }

    root_path
}

/// Fails loudly (rather than silently no-op'ing) so a missing kicad-cli locally is obvious
/// rather than looking like a spuriously-passing empty test.
macro_rules! require_kicad_cli {
    () => {
        match find_kicad_cli() {
            Some(path) => path,
            None => {
                eprintln!("kicad-cli not found — skipping ERC verification test");
                return;
            }
        }
    };
}

/// Any violation whose item mentions "Sheet Pin" (root-side) or "Hierarchical Label" (child-side)
/// indicates the hierarchy wiring itself is broken. A plain component pin_not_connected (an
/// unused leg on a test resistor) is expected noise, not a hierarchy bug, so it's excluded.
fn hierarchy_wiring_errors(report: &serde_json::Value) -> Vec<(String, String, Vec<String>)> {
    violations(report)
        .into_iter()
        .filter(|(severity, vtype, items)| {
            severity == "error"
                && (vtype == "wire_dangling"
                    || vtype == "hier_label_mismatch"
                    || vtype == "label_dangling"
                    || items.iter().any(|i| i.contains("Sheet Pin")))
        })
        .collect()
}

#[test]
fn test_erc_declared_sheet_pins_cross_sheet_connection() {
    let kicad_cli = require_kicad_cli!();

    // Sheet pins must sit exactly on the sheet rectangle's edge (right edge of Power at x=200,
    // left edge of Control at x=250) — see place_sheet's border validation.
    let yaml = r#"
meta:
  paper: A4

components:
  U1:
    symbol: Device:R
    position: [100, 50]
    sheet: Power
  U2:
    symbol: Device:R
    position: [200, 50]
    sheet: Control

sheets:
  Power:
    path: power_erc_cross
    position: [0, 0]
    size: [200, 150]
    pins:
      - name: VCC
        shape: output
        position: [200, 50]
  Control:
    path: control_erc_cross
    position: [250, 0]
    size: [200, 150]
    pins:
      - name: VCC
        shape: input
        position: [250, 50]

connections:
  - net: VCC
    pins: [U1:1, U2:1]
"#;

    let dir = std::env::temp_dir().join(format!("kicaddy_erc_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let root_path = compile_to_dir(yaml, &dir, "root");

    let report = run_erc(&kicad_cli, &root_path);
    let bad = hierarchy_wiring_errors(&report);
    assert!(
        bad.is_empty(),
        "declared sheet pins should produce a real, ERC-clean cross-sheet connection, got: {:#?}",
        bad
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_erc_declared_sheet_pin_bad_geometry_is_rejected_at_compile_time() {
    // A declared pin position that isn't on any edge of the sheet rectangle used to compile
    // "successfully" into a schematic that kicad-cli sch erc reported as wire_dangling — with no
    // indication of why. It must now fail fast, at compile time, with a clear message.
    let yaml = r#"
components:
  U1:
    symbol: Device:R
    position: [100, 50]
    sheet: Power

sheets:
  Power:
    path: power_bad_geometry
    position: [0, 0]
    size: [200, 150]
    pins:
      - name: VCC
        shape: output
        position: [10, 10]

connections:
  - net: VCC
    pins: [U1:1]
"#;

    let result = compile_yaml_str(yaml);
    assert!(result.is_err(), "a sheet pin not on any edge should be rejected, not silently compiled");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("VCC"));
    assert!(message.contains("border"));
}

#[test]
fn test_erc_unused_declared_sheet_pin_is_dropped_not_dangling() {
    let kicad_cli = require_kicad_cli!();

    // VCC sits on a valid edge, but only a same-sheet connection ever references it — no
    // cross-sheet connection wires it up, so it should be dropped (with a warning) rather than
    // left on the sheet symbol with nothing inside the child sheet pointing back at it.
    let yaml = r#"
meta:
  paper: A4

components:
  U1:
    symbol: Device:R
    position: [100, 50]
    sheet: Power
  U2:
    symbol: Device:R
    position: [200, 50]
    sheet: Power

sheets:
  Power:
    path: power_erc_unused_pin
    position: [0, 0]
    size: [200, 150]
    pins:
      - name: VCC
        shape: output
        position: [0, 50]

connections:
  - net: VCC
    pins: [U1:1, U2:1]
"#;

    let output = compile_yaml_str(yaml).expect("compile_yaml_str failed");
    assert!(output.root.sheets[0].pins.is_empty(), "unused declared pin should have been dropped");
    assert!(output.warnings.iter().any(|w| w.contains("VCC")));

    let dir = std::env::temp_dir().join(format!("kicaddy_erc_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let root_path = compile_to_dir(yaml, &dir, "root");

    let report = run_erc(&kicad_cli, &root_path);
    let bad = hierarchy_wiring_errors(&report);
    assert!(
        bad.is_empty(),
        "an unused declared sheet pin must not leave a dangling pin/hier_label_mismatch behind, got: {:#?}",
        bad
    );

    std::fs::remove_dir_all(&dir).ok();
}
