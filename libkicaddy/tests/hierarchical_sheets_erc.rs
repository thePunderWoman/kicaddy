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

/// Grid-snap a value to the same 1.27mm grid the compiler snaps to, so generated sheet/pin
/// geometry lands exactly on-edge (see place_sheet's border validation).
fn snap(v: f64) -> f64 {
    (v / 1.27).round() * 1.27
}

/// Build a yaml schematic with `n` independently-declared nets crossing between two sheets
/// (`SideA`'s bottom edge to `SideB`'s top edge), evenly spaced along a shared-width edge. If
/// `duplicate_pins_net` is `Some(i)`, that net gets 2 extra same-sheet pins on SideB's side
/// (mimicking a component pin plus a local pull-up/decoupling part sharing the net locally) —
/// this is the shape of the real-world I2C_SCL/I2C_SDA/WL_ON case.
fn generate_dense_edge_yaml(n: usize, duplicate_pins_net: Option<usize>) -> String {
    let sheet_w = snap(700.0);
    let sheet_h = snap(150.0);
    let margin = 20.0;
    let span = sheet_w - 2.0 * margin;
    let gap = snap(100.0);

    let x_at = |i: usize| snap(margin + span * (i as f64 + 0.5) / n as f64);

    let mut yaml = String::from("meta:\n  paper: A4\n\ncomponents:\n");
    for i in 0..n {
        let x = x_at(i);
        yaml += &format!(
            "  UA{i}:\n    symbol: Device:R\n    position: [{x}, 50]\n    sheet: SideA\n"
        );
        yaml += &format!(
            "  UB{i}:\n    symbol: Device:R\n    position: [{x}, 50]\n    sheet: SideB\n"
        );
    }
    if let Some(dup_i) = duplicate_pins_net {
        assert!(dup_i < n);
        let x = x_at(dup_i);
        yaml += &format!(
            "  UB{dup_i}_pullup:\n    symbol: Device:R\n    position: [{x}, 90]\n    sheet: SideB\n"
        );
        yaml += &format!(
            "  UB{dup_i}_decouple:\n    symbol: Device:R\n    position: [{x}, 120]\n    sheet: SideB\n"
        );
    }

    yaml += &format!(
        "\nsheets:\n  SideA:\n    path: side_a_dense\n    position: [0, 0]\n    size: [{sheet_w}, {sheet_h}]\n    pins:\n"
    );
    for i in 0..n {
        yaml += &format!(
            "      - name: NET{i}\n        shape: bidirectional\n        position: [{}, {sheet_h}]\n",
            x_at(i)
        );
    }
    let side_b_x = sheet_w + gap;
    yaml += &format!(
        "  SideB:\n    path: side_b_dense\n    position: [{side_b_x}, 0]\n    size: [{sheet_w}, {sheet_h}]\n    pins:\n"
    );
    for i in 0..n {
        yaml += &format!(
            "      - name: NET{i}\n        shape: bidirectional\n        position: [{}, 0]\n",
            snap(side_b_x + x_at(i))
        );
    }

    yaml += "\nconnections:\n";
    for i in 0..n {
        yaml += &format!("  - net: NET{i}\n    pins: [UA{i}:1, UB{i}:1");
        if duplicate_pins_net == Some(i) {
            yaml += &format!(", UB{i}_pullup:1, UB{i}_decouple:1");
        }
        yaml += "]\n";
    }

    yaml
}

#[test]
fn test_erc_dense_pin_packing_on_one_edge() {
    let kicad_cli = require_kicad_cli!();

    // 19 independent nets, all pins evenly spaced along the same 700-unit sheet edge — matches
    // the shape that first surfaced this bug (MCU_Core's 19-pin bottom edge in the real
    // SnipsControllers migration). Root cause: Orthogonal routing's L-shaped hub-and-spoke wire
    // ran its horizontal leg along the shared edge for every net, so unrelated nets' wires
    // overlapped collinearly over a wide span; KiCAD's ERC then non-deterministically
    // misattributed connectivity for a fraction of them. Fixed by switching to Direct routing.
    let yaml = generate_dense_edge_yaml(19, None);

    let dir = std::env::temp_dir().join(format!("kicaddy_erc_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let root_path = compile_to_dir(&yaml, &dir, "root");

    let report = run_erc(&kicad_cli, &root_path);
    let bad = hierarchy_wiring_errors(&report);
    assert!(
        bad.is_empty(),
        "19 nets densely packed on one sheet edge should all be ERC-clean, got: {:#?}",
        bad
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_erc_same_sheet_duplicate_pins_within_dense_packing() {
    let kicad_cli = require_kicad_cli!();

    // A net with 2+ participating pins on the same (non-primary) sheet — e.g. an IC pin plus a
    // local pull-up resistor pin — creates a separate hierarchical_label per local pin rather
    // than joining them first. That pattern alone is electrically harmless (KiCAD treats
    // same-named hierarchical labels on one screen as joined, like ordinary labels); it was
    // originally misdiagnosed as a distinct bug because it was tested only inside dense edge
    // packing, where the real cause (see test_erc_dense_pin_packing_on_one_edge) was already
    // failing a fraction of *every* net on that edge, duplicate-pin or not. This test mixes both
    // shapes into one repro to guard against that conflation recurring.
    let yaml = generate_dense_edge_yaml(12, Some(5));

    let dir = std::env::temp_dir().join(format!("kicaddy_erc_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let root_path = compile_to_dir(&yaml, &dir, "root");

    let report = run_erc(&kicad_cli, &root_path);
    let bad = hierarchy_wiring_errors(&report);
    assert!(
        bad.is_empty(),
        "a net with duplicate same-sheet pins mixed into dense edge packing should be ERC-clean, got: {:#?}",
        bad
    );

    std::fs::remove_dir_all(&dir).ok();
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
