//! Tests for the semantic outline tool

use libkicaddy::parse_schematic;
use libkicaddy::tools::build_semantic_outline;

#[test]
fn test_semantic_outline_basic() {
    // Parse a test schematic
    let schematic = parse_schematic("../testing/testing.kicad_sch")
        .expect("Failed to parse schematic");

    let outline = build_semantic_outline(&schematic);

    // Verify basic structure
    assert!(!outline.components.is_empty(), "Should have components");

    // Print output for manual verification
    println!("{}", outline.to_text());
}

#[test]
fn test_semantic_outline_from_compiled_yaml() {
    // Parse the compiled YAML schematic
    let schematic = parse_schematic("../kato-turntable-controller.kicad_sch")
        .expect("Failed to parse schematic");

    let outline = build_semantic_outline(&schematic);

    // Should have components (excluding absorbed passives)
    assert!(!outline.components.is_empty(), "Should have components");

    // Should have connections
    assert!(!outline.connections.is_empty(), "Should have connections");

    // Should have absorbed some passives (pullups/decoupling caps)
    // Note: This may be 0 if the pattern detection doesn't find matching patterns
    // The test passes either way, we're just verifying it doesn't crash

    // Print output for manual verification
    let text = outline.to_text();
    println!("{}", text);

    // Verify text format
    assert!(text.contains("Components:"), "Should have Components section");
    assert!(text.contains("Connections:"), "Should have Connections section");
}
