//! Integration tests for schematic parsing

use libkicaddy::parser::sexpr::ToSExpr;
use libkicaddy::schematic::{parse_schematic, parse_schematic_str, Schematic};

// Get the workspace root (one level up from libkicaddy)
fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn test_parse_bilge_pump_schematic() {
    let path = workspace_root().join("SparkFun_Thing_Plus_ESP32-S3/BilgePump/BilgePump.kicad_sch");

    let sch = parse_schematic(path).expect("Failed to parse BilgePump schematic");

    // Verify basic schematic properties
    assert!(sch.version > 0, "Version should be set");
    assert!(sch.generator.is_some(), "Generator should be set");
    assert!(!sch.uuid.is_empty(), "UUID should be set");

    // Verify lib_symbols were parsed
    assert!(!sch.lib_symbols.is_empty(), "Should have lib_symbols");
    println!(
        "BilgePump: {} lib_symbols, {} symbols, {} wires, {} junctions",
        sch.lib_symbols.len(),
        sch.symbols.len(),
        sch.wires.len(),
        sch.junctions.len()
    );

    // Verify some symbol instances were parsed
    assert!(!sch.symbols.is_empty(), "Should have symbol instances");

    // Verify wires were parsed
    assert!(!sch.wires.is_empty(), "Should have wires");

    // Verify junctions were parsed
    assert!(!sch.junctions.is_empty(), "Should have junctions");
}

#[test]
fn test_parse_main_schematic() {
    let path = workspace_root().join("SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3.kicad_sch");

    let sch = parse_schematic(path).expect("Failed to parse main schematic");

    // Verify basic schematic properties
    assert!(sch.version > 0, "Version should be set");
    assert!(sch.title_block.is_some(), "Should have title block");

    let tb = sch.title_block.as_ref().unwrap();
    assert!(tb.title.is_some(), "Title should be set");
    println!("Main schematic title: {:?}", tb.title);

    // Verify global labels were parsed
    assert!(!sch.global_labels.is_empty(), "Should have global labels");
    println!("Global labels count: {}", sch.global_labels.len());

    // Verify text items were parsed
    assert!(!sch.text_items.is_empty(), "Should have text items");
    println!("Text items count: {}", sch.text_items.len());

    println!(
        "Main schematic: {} lib_symbols, {} symbols, {} wires, {} junctions, {} global_labels",
        sch.lib_symbols.len(),
        sch.symbols.len(),
        sch.wires.len(),
        sch.junctions.len(),
        sch.global_labels.len()
    );
}

#[test]
fn test_parse_peripherals_schematic() {
    let path = workspace_root().join("SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3_Peripherals.kicad_sch");

    let sch = parse_schematic(path).expect("Failed to parse peripherals schematic");

    // Verify basic schematic properties
    assert!(sch.version > 0, "Version should be set");

    println!(
        "Peripherals: {} lib_symbols, {} symbols, {} wires, {} junctions",
        sch.lib_symbols.len(),
        sch.symbols.len(),
        sch.wires.len(),
        sch.junctions.len()
    );
}

#[test]
fn test_parse_all_schematics() {
    let paths = [
        "SparkFun_Thing_Plus_ESP32-S3/BilgePump/BilgePump.kicad_sch",
        "SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3.kicad_sch",
        "SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3_Peripherals.kicad_sch",
    ];

    for rel_path in paths {
        let path = workspace_root().join(rel_path);
        let result = parse_schematic(&path);
        assert!(
            result.is_ok(),
            "Failed to parse {}: {:?}",
            rel_path,
            result.err()
        );
        println!("Successfully parsed: {}", rel_path);
    }
}

/// Helper to compare two schematics structurally
fn assert_schematics_equal(original: &Schematic, reparsed: &Schematic, context: &str) {
    // Compare metadata
    assert_eq!(original.version, reparsed.version, "{}: version mismatch", context);
    assert_eq!(original.generator, reparsed.generator, "{}: generator mismatch", context);
    assert_eq!(original.uuid, reparsed.uuid, "{}: uuid mismatch", context);
    assert_eq!(original.paper, reparsed.paper, "{}: paper mismatch", context);

    // Compare element counts
    assert_eq!(
        original.lib_symbols.len(),
        reparsed.lib_symbols.len(),
        "{}: lib_symbols count mismatch",
        context
    );
    assert_eq!(
        original.symbols.len(),
        reparsed.symbols.len(),
        "{}: symbols count mismatch",
        context
    );
    assert_eq!(
        original.wires.len(),
        reparsed.wires.len(),
        "{}: wires count mismatch",
        context
    );
    assert_eq!(
        original.junctions.len(),
        reparsed.junctions.len(),
        "{}: junctions count mismatch",
        context
    );
    assert_eq!(
        original.buses.len(),
        reparsed.buses.len(),
        "{}: buses count mismatch",
        context
    );
    assert_eq!(
        original.bus_entries.len(),
        reparsed.bus_entries.len(),
        "{}: bus_entries count mismatch",
        context
    );
    assert_eq!(
        original.no_connects.len(),
        reparsed.no_connects.len(),
        "{}: no_connects count mismatch",
        context
    );
    assert_eq!(
        original.global_labels.len(),
        reparsed.global_labels.len(),
        "{}: global_labels count mismatch",
        context
    );
    assert_eq!(
        original.hierarchical_labels.len(),
        reparsed.hierarchical_labels.len(),
        "{}: hierarchical_labels count mismatch",
        context
    );
    assert_eq!(
        original.labels.len(),
        reparsed.labels.len(),
        "{}: labels count mismatch",
        context
    );
    assert_eq!(
        original.text_items.len(),
        reparsed.text_items.len(),
        "{}: text_items count mismatch",
        context
    );

    // Compare title block presence
    assert_eq!(
        original.title_block.is_some(),
        reparsed.title_block.is_some(),
        "{}: title_block presence mismatch",
        context
    );

    // Compare sheet instances count
    assert_eq!(
        original.sheet_instances.len(),
        reparsed.sheet_instances.len(),
        "{}: sheet_instances count mismatch",
        context
    );

    // Compare symbol details (UUIDs, lib_ids)
    for (i, (orig, rep)) in original.symbols.iter().zip(reparsed.symbols.iter()).enumerate() {
        assert_eq!(orig.uuid, rep.uuid, "{}: symbol[{}] uuid mismatch", context, i);
        assert_eq!(orig.lib_id, rep.lib_id, "{}: symbol[{}] lib_id mismatch", context, i);
        assert_eq!(
            orig.properties.len(),
            rep.properties.len(),
            "{}: symbol[{}] properties count mismatch",
            context,
            i
        );
    }

    // Compare wire details (UUIDs, positions)
    for (i, (orig, rep)) in original.wires.iter().zip(reparsed.wires.iter()).enumerate() {
        assert_eq!(orig.uuid, rep.uuid, "{}: wire[{}] uuid mismatch", context, i);
        assert_eq!(orig.points, rep.points, "{}: wire[{}] points mismatch", context, i);
    }

    // Compare junction details
    for (i, (orig, rep)) in original.junctions.iter().zip(reparsed.junctions.iter()).enumerate() {
        assert_eq!(orig.uuid, rep.uuid, "{}: junction[{}] uuid mismatch", context, i);
        assert_eq!(orig.position, rep.position, "{}: junction[{}] position mismatch", context, i);
    }
}

#[test]
fn test_roundtrip_bilge_pump() {
    let path = workspace_root().join("SparkFun_Thing_Plus_ESP32-S3/BilgePump/BilgePump.kicad_sch");
    let original = parse_schematic(&path).expect("Failed to parse original");

    // Serialize to string
    let serialized = original.to_sexpr().to_kicad_string();

    // Re-parse
    let reparsed = parse_schematic_str(&serialized).expect("Failed to re-parse serialized output");

    // Compare
    assert_schematics_equal(&original, &reparsed, "BilgePump");
    println!(
        "BilgePump round-trip: {} symbols, {} wires, {} junctions verified",
        original.symbols.len(),
        original.wires.len(),
        original.junctions.len()
    );
}

#[test]
fn test_roundtrip_main_schematic() {
    let path = workspace_root().join("SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3.kicad_sch");
    let original = parse_schematic(&path).expect("Failed to parse original");

    // Serialize to string
    let serialized = original.to_sexpr().to_kicad_string();

    // Re-parse
    let reparsed = parse_schematic_str(&serialized).expect("Failed to re-parse serialized output");

    // Compare
    assert_schematics_equal(&original, &reparsed, "MainSchematic");
    println!(
        "Main schematic round-trip: {} symbols, {} wires, {} global_labels verified",
        original.symbols.len(),
        original.wires.len(),
        original.global_labels.len()
    );
}

#[test]
fn test_roundtrip_peripherals() {
    let path = workspace_root().join("SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3_Peripherals.kicad_sch");
    let original = parse_schematic(&path).expect("Failed to parse original");

    // Serialize to string
    let serialized = original.to_sexpr().to_kicad_string();

    // Re-parse
    let reparsed = parse_schematic_str(&serialized).expect("Failed to re-parse serialized output");

    // Compare
    assert_schematics_equal(&original, &reparsed, "Peripherals");
    println!(
        "Peripherals round-trip: {} symbols, {} wires, {} junctions verified",
        original.symbols.len(),
        original.wires.len(),
        original.junctions.len()
    );
}

#[test]
fn test_roundtrip_all_schematics() {
    let paths = [
        "SparkFun_Thing_Plus_ESP32-S3/BilgePump/BilgePump.kicad_sch",
        "SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3.kicad_sch",
        "SparkFun_Thing_Plus_ESP32-S3/SparkFun_Thing_Plus_ESP32-S3_Peripherals.kicad_sch",
    ];

    for rel_path in paths {
        let path = workspace_root().join(rel_path);
        let original = parse_schematic(&path).expect(&format!("Failed to parse {}", rel_path));

        // Serialize to string
        let serialized = original.to_sexpr().to_kicad_string();

        // Re-parse
        let reparsed = parse_schematic_str(&serialized)
            .expect(&format!("Failed to re-parse serialized {}", rel_path));

        // Compare
        assert_schematics_equal(&original, &reparsed, rel_path);
        println!("Round-trip verified: {}", rel_path);
    }
}
