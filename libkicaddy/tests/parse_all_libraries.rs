//! Integration test that validates parsing against all installed KiCad symbol libraries

use std::fs;

use libkicaddy::{KicadConfig, parse_symbol_library};

/// Test that all KiCad symbol libraries can be parsed successfully
///
/// This test is slow (~50s) so it's ignored by default.
/// Run with: `cargo test -- --ignored`
#[test]
#[ignore]
fn parse_all_kicad_symbol_libraries() {
    let config = match KicadConfig::detect() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Skipping test: KiCad not found ({e})");
            return;
        }
    };

    let symbols_dir = &config.symbol_lib_path;
    if !symbols_dir.exists() {
        eprintln!("Skipping test: Symbol library path does not exist: {}", symbols_dir.display());
        return;
    }

    let mut total = 0;
    let mut passed = 0;
    let mut failed = Vec::new();

    // Walk the directory and find all .kicad_sym files
    let entries = match fs::read_dir(symbols_dir) {
        Ok(entries) => entries,
        Err(e) => {
            panic!("Failed to read symbol library directory: {e}");
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "kicad_sym").unwrap_or(false) {
            total += 1;
            match parse_symbol_library(&path) {
                Ok(lib) => {
                    passed += 1;
                    println!(
                        "OK: {} ({} symbols)",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        lib.symbols.len()
                    );
                }
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    failed.push((filename.clone(), format!("{e}")));
                    eprintln!("FAIL: {filename}: {e}");
                }
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total: {total}");
    println!("Passed: {passed}");
    println!("Failed: {}", failed.len());

    if !failed.is_empty() {
        println!("\nFailed libraries:");
        for (name, error) in &failed {
            println!("  - {name}");
            // Print first line of error only to keep output manageable
            if let Some(first_line) = error.lines().next() {
                println!("    {first_line}");
            }
        }
        panic!(
            "{} out of {total} libraries failed to parse",
            failed.len()
        );
    }
}
