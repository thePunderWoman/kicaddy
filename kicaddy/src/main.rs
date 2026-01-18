mod mcp;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use libkicaddy::commands::{
    parse_label_shape, parse_pin_ref, parse_routing_mode, AddLabelCommand, AddWireCommand,
    Command, DeleteComponentCommand, DeleteLabelCommand, DeleteWireCommand, LabelLocation,
    PlaceComponentCommand, WireEndpoint,
};
use libkicaddy::common::{Point, Position};
use libkicaddy::parser::sexpr::ToSExpr;
use libkicaddy::schematic::Schematic;
use libkicaddy::tools::{
    OutlineInput, OutlineTool, Tool, UpdateComponentInput, UpdateComponentTool,
};
use libkicaddy::{
    build_index, find_symbol, parse_schematic, parse_symbol_library, search, KicadConfig,
    SearchOptions, YamlSchematic, YamlTemplate,
};

#[derive(Parser)]
#[command(name = "kicaddy")]
#[command(about = "CLI tool for manipulating KiCAD files", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show detected KiCAD configuration
    Config,
    /// List available symbol libraries
    ListLibraries {
        /// Show symbol count for each library (slower)
        #[arg(short, long)]
        verbose: bool,
    },
    /// List symbols in a library file
    ListSymbols {
        /// Path to the .kicad_sym file
        path: PathBuf,
        /// Show detailed information for each symbol
        #[arg(short, long)]
        verbose: bool,
    },
    /// Parse a schematic and dump it back to stdout (for debugging round-trip)
    DumpSchematic {
        /// Path to the .kicad_sch file
        path: PathBuf,
    },
    /// Create a new empty schematic file
    NewSchematic {
        /// Path for the new .kicad_sch file
        path: PathBuf,
    },
    /// Place a component from a KiCAD symbol library into a schematic
    PlaceComponent {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// Library name (e.g., "Device")
        #[arg(short, long)]
        library: String,
        /// Symbol name within the library (e.g., "R")
        #[arg(short, long)]
        symbol: String,
        /// X position in schematic units
        #[arg(long)]
        x: f64,
        /// Y position in schematic units
        #[arg(long)]
        y: f64,
        /// Rotation in degrees (default: 0)
        #[arg(short, long, default_value = "0")]
        angle: f64,
        /// Reference designator (e.g., "R1"). Default: auto from symbol
        #[arg(short, long)]
        reference: Option<String>,
        /// Component value. Default: symbol name
        #[arg(short, long)]
        value: Option<String>,
    },
    /// Build or rebuild the symbol search index
    Index,
    /// Search for symbols in the index
    Search {
        /// Search query (e.g., "current sensor i2c")
        query: String,
        /// Maximum number of results (default: 10)
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
        /// Filter to a specific library
        #[arg(short, long)]
        library: Option<String>,
        /// Output results as JSON (for MCP integration)
        #[arg(short, long)]
        json: bool,
    },
    /// Add a wire connection to a schematic
    AddWire {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// Start pin in "REF:PIN" format (e.g., "R1:1")
        #[arg(long)]
        from: Option<String>,
        /// End pin in "REF:PIN" format (e.g., "U1:VCC")
        #[arg(long)]
        to: Option<String>,
        /// Start X coordinate (alternative to --from)
        #[arg(long)]
        x1: Option<f64>,
        /// Start Y coordinate (alternative to --from)
        #[arg(long)]
        y1: Option<f64>,
        /// End X coordinate (alternative to --to)
        #[arg(long)]
        x2: Option<f64>,
        /// End Y coordinate (alternative to --to)
        #[arg(long)]
        y2: Option<f64>,
        /// Routing mode: direct, orthogonal, or orthogonal-vh
        #[arg(long, default_value = "direct")]
        routing: String,
    },
    /// Add a label to a schematic
    AddLabel {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// Label text
        #[arg(short = 'n', long)]
        name: String,
        /// Pin in "REF:PIN" format (e.g., "R1:1")
        #[arg(long)]
        pin: Option<String>,
        /// X coordinate (alternative to --pin)
        #[arg(long)]
        x: Option<f64>,
        /// Y coordinate (alternative to --pin)
        #[arg(long)]
        y: Option<f64>,
        /// Label angle in degrees (auto-detected if --pin is used)
        #[arg(long, default_value = "0")]
        angle: f64,
        /// Use global label instead of local
        #[arg(long)]
        global: bool,
        /// Shape for global label: input, output, bidirectional, tri_state, passive
        #[arg(long, default_value = "input")]
        shape: String,
    },
    /// Delete a component from a schematic (also removes connected wires and labels at pins)
    DeleteComponent {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// Reference designator of the component to delete (e.g., "R1")
        reference: String,
    },
    /// Delete a wire from a schematic
    DeleteWire {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// X coordinate of a point on the wire
        #[arg(long)]
        x: f64,
        /// Y coordinate of a point on the wire
        #[arg(long)]
        y: f64,
    },
    /// Delete a label from a schematic
    DeleteLabel {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// Label text to delete
        #[arg(short = 'n', long)]
        name: String,
        /// X coordinate (optional, to disambiguate multiple labels with same name)
        #[arg(long)]
        x: Option<f64>,
        /// Y coordinate (optional, to disambiguate multiple labels with same name)
        #[arg(long)]
        y: Option<f64>,
    },
    /// Get outline view of schematic (components, pins, nets)
    Outline {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// Output as JSON (for MCP integration)
        #[arg(short, long)]
        json: bool,
    },
    /// Update a component's properties (position, angle, reference, value, mirror)
    UpdateComponent {
        /// Path to the .kicad_sch file
        schematic: PathBuf,
        /// Reference designator of component to update (e.g., "R1")
        reference: String,
        /// New X position
        #[arg(long)]
        x: Option<f64>,
        /// New Y position
        #[arg(long)]
        y: Option<f64>,
        /// New rotation angle in degrees
        #[arg(long)]
        angle: Option<f64>,
        /// New reference designator (to rename component)
        #[arg(long)]
        new_reference: Option<String>,
        /// New value
        #[arg(long)]
        value: Option<String>,
        /// Mirror setting: "x", "y", or "none"
        #[arg(long)]
        mirror: Option<String>,
    },
    /// Start MCP server for AI assistant integration
    Mcp,
    /// Compile a YAML schematic definition to KiCAD format
    Compile {
        /// Path to the YAML schematic definition file
        yaml: PathBuf,
        /// Output path for the .kicad_sch file (default: same name with .kicad_sch extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Only validate the YAML file without compiling
        #[arg(long)]
        validate: bool,
    },
    /// Create a new YAML schematic template
    InitYaml {
        /// Path for the new YAML file
        path: PathBuf,
        /// Template to use: basic, regulator, led (default: basic)
        #[arg(short, long, default_value = "basic")]
        template: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config => {
            match KicadConfig::detect() {
                Ok(config) => {
                    println!("KiCAD Configuration:");
                    println!("  Installation: {}", config.kicad_path.display());
                    println!("  Symbols:      {}", config.symbol_lib_path.display());
                    println!("  Footprints:   {}", config.footprint_lib_path.display());
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::ListLibraries { verbose } => {
            let config = match KicadConfig::detect() {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let entries = match std::fs::read_dir(&config.symbol_lib_path) {
                Ok(entries) => entries,
                Err(e) => {
                    eprintln!("Error reading symbol library directory: {}", e);
                    std::process::exit(1);
                }
            };

            let mut libraries: Vec<_> = entries
                .flatten()
                .filter(|e| {
                    e.path().extension().map(|ext| ext == "kicad_sym").unwrap_or(false)
                })
                .collect();

            libraries.sort_by_key(|e| e.file_name());

            println!("Symbol libraries in {}:\n", config.symbol_lib_path.display());

            for entry in libraries {
                let path = entry.path();
                let name = path.file_stem().unwrap_or_default().to_string_lossy();

                if verbose {
                    match parse_symbol_library(&path) {
                        Ok(lib) => println!("{} ({} symbols)", name, lib.symbols.len()),
                        Err(_) => println!("{} (parse error)", name),
                    }
                } else {
                    println!("{}", name);
                }
            }
        }
        Commands::ListSymbols { path, verbose } => {
            match parse_symbol_library(&path) {
                Ok(lib) => {
                    println!(
                        "Library: {} (version {}, {} symbols)",
                        path.display(),
                        lib.version,
                        lib.symbols.len()
                    );
                    println!();

                    for symbol in &lib.symbols {
                        if verbose {
                            println!("{}:", symbol.name);
                            if let Some(desc) = symbol.description() {
                                println!("  Description: {}", desc);
                            }
                            if let Some(ref_) = symbol.reference() {
                                println!("  Reference: {}", ref_);
                            }
                            if let Some(fp) = symbol.footprint() {
                                if !fp.is_empty() {
                                    println!("  Footprint: {}", fp);
                                }
                            }
                            let pin_count: usize = symbol.units.iter().map(|u| u.pins.len()).sum();
                            println!("  Units: {}, Pins: {}", symbol.units.len(), pin_count);
                            println!();
                        } else {
                            println!("{}", symbol.name);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error parsing library: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::DumpSchematic { path } => {
            match parse_schematic(&path) {
                Ok(schematic) => {
                    let output = schematic.to_sexpr().to_kicad_string();
                    println!("{}", output);
                }
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::NewSchematic { path } => {
            let schematic = Schematic::new();
            match schematic.write_to_file(&path) {
                Ok(()) => {
                    println!("Created new schematic: {}", path.display());
                }
                Err(e) => {
                    eprintln!("Error writing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::PlaceComponent {
            schematic,
            library,
            symbol,
            x,
            y,
            angle,
            reference,
            value,
        } => {
            // Detect KiCAD configuration
            let config = match KicadConfig::detect() {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Look up the symbol from the library
            let sym = match find_symbol(&config, &library, &symbol) {
                Ok(sym) => sym,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Parse the existing schematic
            let mut sch = match parse_schematic(&schematic) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
                    std::process::exit(1);
                }
            };

            // Create and execute the command
            let lib_id = format!("{}:{}", library, symbol);
            let cmd = PlaceComponentCommand {
                symbol: sym,
                lib_id: lib_id.clone(),
                position: Position::new(x, y, angle),
                reference: reference.clone(),
                value,
            };

            let result = match cmd.execute(&mut sch) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Write the schematic back to the file
            match sch.write_to_file(&schematic) {
                Ok(()) => {
                    if result.was_snapped {
                        println!(
                            "Placed {} ({}) at ({}, {}) [snapped from ({}, {})] in {}",
                            lib_id,
                            result.reference,
                            result.x,
                            result.y,
                            x,
                            y,
                            schematic.display()
                        );
                    } else {
                        println!(
                            "Placed {} ({}) at ({}, {}) in {}",
                            lib_id,
                            result.reference,
                            result.x,
                            result.y,
                            schematic.display()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error writing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Index => {
            let config = match KicadConfig::detect() {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            println!("Building search index...");
            for path in config.all_symbol_lib_paths() {
                println!("  {}", path.display());
            }

            match build_index(&config) {
                Ok(stats) => {
                    println!(
                        "Indexed {} symbols from {} libraries in {}ms",
                        stats.symbols_indexed, stats.libraries_indexed, stats.elapsed_ms
                    );
                    if stats.libraries_failed > 0 {
                        eprintln!("Warning: {} libraries failed to parse", stats.libraries_failed);
                    }
                }
                Err(e) => {
                    eprintln!("Error building index: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Search {
            query,
            limit,
            library,
            json,
        } => {
            let mut options = SearchOptions::new().with_limit(limit);
            if let Some(lib) = library {
                options = options.with_library_filter(lib);
            }

            match search(&query, options) {
                Ok(results) => {
                    if json {
                        // JSON output for MCP integration
                        match serde_json::to_string_pretty(&results) {
                            Ok(json_str) => println!("{}", json_str),
                            Err(e) => {
                                eprintln!("Error serializing results: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // Human-readable output
                        if results.results.is_empty() {
                            println!("No results found for \"{}\"", query);
                        } else {
                            println!(
                                "Found {} results for \"{}\" ({}ms):\n",
                                results.total_hits, query, results.query_time_ms
                            );
                            for result in &results.results {
                                println!("  {} ({})", result.lib_id, result.reference);
                                if !result.description.is_empty() {
                                    println!("    {}", result.description);
                                }
                                if !result.keywords.is_empty() {
                                    println!("    Keywords: {}", result.keywords);
                                }
                                println!("    Score: {:.2}", result.score);
                                println!();
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::AddWire {
            schematic,
            from,
            to,
            x1,
            y1,
            x2,
            y2,
            routing,
        } => {
            // Parse the schematic
            let mut sch = match parse_schematic(&schematic) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
                    std::process::exit(1);
                }
            };

            // Parse routing mode
            let mode = match parse_routing_mode(&routing) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Determine start endpoint
            let from_endpoint = match (from, x1, y1) {
                (Some(pin_ref), _, _) => {
                    let (r, p) = match parse_pin_ref(&pin_ref) {
                        Ok(rp) => rp,
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    };
                    WireEndpoint::Pin { reference: r, pin: p }
                }
                (None, Some(x), Some(y)) => WireEndpoint::Point(Point::new(x, y)),
                _ => {
                    eprintln!("Error: Must specify either --from or --x1/--y1");
                    std::process::exit(1);
                }
            };

            // Determine end endpoint
            let to_endpoint = match (to, x2, y2) {
                (Some(pin_ref), _, _) => {
                    let (r, p) = match parse_pin_ref(&pin_ref) {
                        Ok(rp) => rp,
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    };
                    WireEndpoint::Pin { reference: r, pin: p }
                }
                (None, Some(x), Some(y)) => WireEndpoint::Point(Point::new(x, y)),
                _ => {
                    eprintln!("Error: Must specify either --to or --x2/--y2");
                    std::process::exit(1);
                }
            };

            // Create and execute the command
            let cmd = AddWireCommand {
                from: from_endpoint,
                to: to_endpoint,
                routing: mode,
            };

            let result = match cmd.execute(&mut sch) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Write back
            match sch.write_to_file(&schematic) {
                Ok(()) => {
                    println!(
                        "Added wire from ({:.2}, {:.2}) to ({:.2}, {:.2}) in {}",
                        result.from.x,
                        result.from.y,
                        result.to.x,
                        result.to.y,
                        schematic.display()
                    );
                }
                Err(e) => {
                    eprintln!("Error writing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::AddLabel {
            schematic,
            name,
            pin,
            x,
            y,
            angle,
            global,
            shape,
        } => {
            // Parse the schematic
            let mut sch = match parse_schematic(&schematic) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
                    std::process::exit(1);
                }
            };

            // Parse label shape
            let label_shape = match parse_label_shape(&shape) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Determine location
            let location = if let Some(pin_ref) = pin {
                let (reference, pin_name) = match parse_pin_ref(&pin_ref) {
                    Ok(rp) => rp,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                LabelLocation::Pin {
                    reference,
                    pin: pin_name,
                }
            } else if let (Some(lx), Some(ly)) = (x, y) {
                LabelLocation::Position(Position::new(lx, ly, angle))
            } else {
                eprintln!("Error: Must specify either --pin or --x/--y");
                std::process::exit(1);
            };

            // Create and execute the command
            let cmd = AddLabelCommand {
                text: name.clone(),
                location,
                global,
                shape: label_shape,
            };

            let result = match cmd.execute(&mut sch) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Write back
            match sch.write_to_file(&schematic) {
                Ok(()) => {
                    let label_type = if result.global { "global label" } else { "label" };
                    println!(
                        "Added {} '{}' at ({:.2}, {:.2}) in {}",
                        label_type,
                        result.text,
                        result.position.x,
                        result.position.y,
                        schematic.display()
                    );
                }
                Err(e) => {
                    eprintln!("Error writing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::DeleteComponent {
            schematic,
            reference,
        } => {
            // Parse the schematic
            let mut sch = match parse_schematic(&schematic) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
                    std::process::exit(1);
                }
            };

            // Create and execute the command
            let cmd = DeleteComponentCommand {
                reference: reference.clone(),
            };

            if let Err(e) = cmd.execute(&mut sch) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }

            // Write back
            match sch.write_to_file(&schematic) {
                Ok(()) => {
                    println!(
                        "Deleted component '{}' (and connected wires/labels) from {}",
                        reference,
                        schematic.display()
                    );
                }
                Err(e) => {
                    eprintln!("Error writing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::DeleteWire { schematic, x, y } => {
            // Parse the schematic
            let mut sch = match parse_schematic(&schematic) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
                    std::process::exit(1);
                }
            };

            // Create and execute the command
            let cmd = DeleteWireCommand {
                point: Point::new(x, y),
            };

            if let Err(e) = cmd.execute(&mut sch) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }

            // Write back
            match sch.write_to_file(&schematic) {
                Ok(()) => {
                    println!(
                        "Deleted wire at ({}, {}) from {}",
                        x,
                        y,
                        schematic.display()
                    );
                }
                Err(e) => {
                    eprintln!("Error writing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::DeleteLabel {
            schematic,
            name,
            x,
            y,
        } => {
            // Parse the schematic
            let mut sch = match parse_schematic(&schematic) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
                    std::process::exit(1);
                }
            };

            // Create and execute the command
            let position = match (x, y) {
                (Some(lx), Some(ly)) => Some(Point::new(lx, ly)),
                _ => None,
            };

            let cmd = DeleteLabelCommand {
                name: name.clone(),
                position,
            };

            if let Err(e) = cmd.execute(&mut sch) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }

            // Write back
            match sch.write_to_file(&schematic) {
                Ok(()) => {
                    if let Some(pos) = position {
                        println!(
                            "Deleted label '{}' at ({}, {}) from {}",
                            name,
                            pos.x,
                            pos.y,
                            schematic.display()
                        );
                    } else {
                        println!("Deleted label '{}' from {}", name, schematic.display());
                    }
                }
                Err(e) => {
                    eprintln!("Error writing schematic: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Outline { schematic, json } => {
            let input = OutlineInput { schematic };
            match OutlineTool::execute(input) {
                Ok(output) => {
                    if json {
                        match serde_json::to_string_pretty(&output) {
                            Ok(json_str) => println!("{}", json_str),
                            Err(e) => {
                                eprintln!("Error serializing output: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // Human-readable output
                        println!("Schematic Outline");
                        println!("=================\n");

                        println!(
                            "Stats: {} components, {} wires, {} nets\n",
                            output.stats.component_count,
                            output.stats.wire_count,
                            output.stats.net_count
                        );

                        println!("Components:");
                        for comp in &output.components {
                            println!(
                                "  {} ({}) = {}",
                                comp.reference, comp.lib_id, comp.value
                            );
                            println!(
                                "    Position: ({:.2}, {:.2}), Angle: {}°",
                                comp.x, comp.y, comp.angle
                            );
                            for pin in &comp.pins {
                                let net_str = pin.net.as_deref().unwrap_or("-");
                                if pin.name != pin.number && pin.name != "~" {
                                    println!(
                                        "    Pin {} ({}): {}",
                                        pin.number, pin.name, net_str
                                    );
                                } else {
                                    println!("    Pin {}: {}", pin.number, net_str);
                                }
                            }
                        }

                        if !output.nets.is_empty() {
                            println!("\nNets:");
                            for net in &output.nets {
                                let global_marker = if net.is_global { " (global)" } else { "" };
                                println!("  {}{}: {}", net.name, global_marker, net.connections.join(", "));
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::UpdateComponent {
            schematic,
            reference,
            x,
            y,
            angle,
            new_reference,
            value,
            mirror,
        } => {
            let input = UpdateComponentInput {
                schematic: schematic.clone(),
                reference: reference.clone(),
                x,
                y,
                angle,
                new_reference,
                value,
                mirror,
            };

            match UpdateComponentTool::execute(input) {
                Ok(output) => {
                    if output.changes.is_empty() {
                        println!("No changes made to '{}'", reference);
                    } else {
                        println!("Updated '{}' in {}:", output.reference, schematic.display());
                        for change in &output.changes {
                            println!("  - {}", change);
                        }
                        if output.was_snapped {
                            println!("  (position snapped to grid)");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Mcp => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            if let Err(e) = rt.block_on(mcp::run_server()) {
                eprintln!("MCP server error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Compile {
            yaml,
            output,
            validate,
        } => {
            // Parse the YAML file
            let yaml_sch = match YamlSchematic::from_file(&yaml) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing YAML: {}", e);
                    std::process::exit(1);
                }
            };

            // Create the compiler
            let compiler = match libkicaddy::yaml::Compiler::new() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            if validate {
                // Validate only
                let result = compiler.validate(&yaml_sch);
                if result.is_valid() {
                    println!("YAML schematic is valid");
                    for warning in &result.warnings {
                        println!("Warning: {}", warning);
                    }
                } else {
                    eprintln!("Validation errors:");
                    for error in &result.errors {
                        eprintln!("  - {}", error);
                    }
                    for warning in &result.warnings {
                        println!("Warning: {}", warning);
                    }
                    std::process::exit(1);
                }
            } else {
                // Compile
                match compiler.compile(&yaml_sch) {
                    Ok(compile_output) => {
                        // Determine output path
                        let output_path = output.unwrap_or_else(|| {
                            yaml.with_extension("kicad_sch")
                        });

                        // Write the schematic
                        if let Err(e) = compile_output.schematic.write_to_file(&output_path) {
                            eprintln!("Error writing schematic: {}", e);
                            std::process::exit(1);
                        }

                        println!("Compiled {} to {}", yaml.display(), output_path.display());
                        println!(
                            "  Components placed: {}",
                            compile_output.components_placed.len()
                        );
                        println!("  Connections made: {}", compile_output.connections_made);

                        for warning in &compile_output.warnings {
                            println!("Warning: {}", warning);
                        }
                    }
                    Err(e) => {
                        eprintln!("Compilation error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::InitYaml { path, template } => {
            // Get the template
            let template_type = match YamlTemplate::from_str(&template) {
                Some(t) => t,
                None => {
                    eprintln!(
                        "Unknown template '{}'. Available: basic, regulator, led",
                        template
                    );
                    std::process::exit(1);
                }
            };

            // Write the template
            match std::fs::write(&path, template_type.content()) {
                Ok(()) => {
                    println!("Created YAML schematic template: {}", path.display());
                }
                Err(e) => {
                    eprintln!("Error writing file: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
