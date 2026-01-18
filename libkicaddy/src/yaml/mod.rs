//! YAML-based declarative schematic definition
//!
//! This module provides a declarative YAML format for defining KiCAD schematics,
//! along with a compiler to generate the actual schematic files.
//!
//! # YAML Schema
//!
//! ```yaml
//! meta:
//!   paper: A4
//!   title: "My Circuit"
//!   author: "kicaddy"
//!
//! groups:
//!   Power Supply:
//!     components:
//!       U1:
//!         symbol: Regulator_Linear:AP2112K-3.3
//!         position: [100, 50]
//!         value: 3.3V
//!     connections:
//!       - net: VIN
//!         pins: [C_IN:1, U1:VIN]
//!
//! components:
//!   R1:
//!     symbol: Device:R
//!     position: [150, 50]
//!     value: 10k
//!
//! connections:
//!   - pins: [U1:VOUT, R1:1]
//!   - net: GND
//!     pins: [R1:2]
//! ```
//!
//! # Example Usage
//!
//! ```ignore
//! use libkicaddy::yaml::{YamlSchematic, Compiler};
//!
//! // Parse YAML
//! let yaml_sch = YamlSchematic::from_file("circuit.yaml")?;
//!
//! // Compile to KiCAD schematic
//! let compiler = Compiler::new()?;
//! let output = compiler.compile(&yaml_sch)?;
//!
//! // Save the schematic
//! output.schematic.write_to_file("circuit.kicad_sch")?;
//! ```

mod compiler;
mod error;
mod types;
mod validation;

// Re-export public types
pub use compiler::{compile_yaml_file, compile_yaml_str, CompileOutput, Compiler};
pub use error::YamlError;
pub use types::{ComponentDef, Connection, Group, Meta, Position2D, YamlSchematic, YamlTemplate};
pub use validation::{validate, validate_deep, ValidationResult};
