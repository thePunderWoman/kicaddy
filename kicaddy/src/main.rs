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
    Tool, UpdateComponentInput, UpdateComponentTool, build_semantic_outline,
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
    /// Get detailed information about a symbol
    SymbolInfo {
        /// Library name (e.g., "Device")
        library: String,
        /// Symbol name (e.g., "R")
        symbol: String,
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
    /// Get semantic outline of schematic (components, connections with pullup/pulldown/decap detection)
    Outline {
        /// Path to .kicad_sch file
        schematic: PathBuf,
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
    /// Print computed layout positions (for debugging layout algorithm)
    PrintLayout {
        /// Path to the YAML schematic definition file
        yaml: PathBuf,
    },
    /// Show netlist from a YAML schematic (which pins connect to which nets)
    Netlist {
        /// Path to the YAML schematic definition file
        yaml: PathBuf,
        /// Filter to show only connections for a specific component (e.g., 'U1')
        #[arg(short, long)]
        filter: Option<String>,
    },
    /// Generate a bill of materials from a YAML schematic
    Bom {
        /// Path to the YAML schematic definition file
        yaml: PathBuf,
        /// Don't group by value (show all instances of same symbol together)
        #[arg(long)]
        no_group_by_value: bool,
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
        Commands::SymbolInfo { library, symbol } => {
            let config = match KicadConfig::detect() {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            match find_symbol(&config, &library, &symbol) {
                Ok(sym) => {
                    println!("Symbol: {}:{}", library, symbol);
                    println!("Reference: {}", sym.reference().unwrap_or("?"));
                    if let Some(desc) = sym.description() {
                        println!("Description: {}", desc);
                    }
                    if let Some(ds) = sym.property("Datasheet").map(|p| &p.value).filter(|v| !v.is_empty() && *v != "~") {
                        println!("Datasheet: {}", ds);
                    }
                    if let Some(fp) = sym.footprint().filter(|v| !v.is_empty() && *v != "~") {
                        println!("Footprint: {}", fp);
                    }
                    if let Some(kw) = sym.keywords() {
                        println!("Keywords: {}", kw);
                    }

                    // Collect unique pins
                    let mut seen = std::collections::HashSet::new();
                    let mut pins: Vec<_> = sym.pins()
                        .filter(|p| seen.insert(p.number.number.clone()))
                        .collect();

                    // Sort pins
                    pins.sort_by(|a, b| {
                        let an: Option<i32> = a.number.number.parse().ok();
                        let bn: Option<i32> = b.number.number.parse().ok();
                        match (an, bn) {
                            (Some(a), Some(b)) => a.cmp(&b),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => a.number.number.cmp(&b.number.number),
                        }
                    });

                    println!("\nPins ({}):", pins.len());
                    for pin in &pins {
                        let name_str = if pin.name.name.is_empty() || pin.name.name == "~" {
                            "".to_string()
                        } else {
                            format!(" ({})", pin.name.name)
                        };
                        println!("  {:>4}{:<20} {}", pin.number.number, name_str, pin.electrical_type.as_str());
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
        Commands::Outline { schematic } => {
            match parse_schematic(&schematic) {
                Ok(sch) => {
                    let outline = build_semantic_outline(&sch);
                    println!("{}", outline.to_text());
                }
                Err(e) => {
                    eprintln!("Error parsing schematic: {}", e);
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
                let output_path = output.unwrap_or_else(|| yaml.with_extension("kicad_sch"));
                // Real KiCad projects derive their `instances` block project name from the
                // .kicad_pro/root schematic file stem; kicaddy has no .kicad_pro of its own, so
                // the output filename is the closest equivalent available here.
                let compiler = match output_path.file_stem().and_then(|s| s.to_str()) {
                    Some(stem) => compiler.with_project_name(stem),
                    None => compiler,
                };

                match compiler.compile(&yaml_sch) {
                    Ok(compile_output) => {
                        if let Err(e) = compile_output.root.write_to_file(&output_path) {
                            eprintln!("Error writing schematic: {}", e);
                            std::process::exit(1);
                        }

                        for (sheet_name, child_schematic) in &compile_output.children {
                            let sheet_file = yaml_sch
                                .sheets
                                .get(sheet_name)
                                .and_then(|sheet| sheet.path.clone())
                                .unwrap_or_else(|| sheet_name.clone());

                            let child_path = output_path.with_file_name(if sheet_file.ends_with(".kicad_sch") {
                                sheet_file
                            } else {
                                format!("{}.kicad_sch", sheet_file)
                            });
                            if let Err(e) = child_schematic.write_to_file(&child_path) {
                                eprintln!("Error writing child sheet '{}': {}", sheet_name, e);
                                std::process::exit(1);
                            }
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
        Commands::PrintLayout { yaml } => {
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

            // Get layout info
            match compiler.get_layout_info(&yaml_sch) {
                Ok(output) => {
                    // Print header
                    println!("Layout Debug Output");
                    println!("==================\n");

                    // Print table header
                    println!(
                        "{:<12} {:>18} {:>14} {:>12}",
                        "Component", "Position (mils)", "Size (mils)", "Group"
                    );
                    println!(
                        "{:<12} {:>18} {:>14} {:>12}",
                        "---------", "---------------", "-----------", "-----"
                    );

                    // Print each component
                    for comp in &output.components {
                        let pos_str = format!("({}, {})", comp.position_mils.0, comp.position_mils.1);
                        let size_str = format!("{} x {}", comp.size_mils.0, comp.size_mils.1);
                        let group_str = comp.group.as_deref().unwrap_or("-");
                        let fixed_marker = if comp.fixed { " *" } else { "" };
                        println!(
                            "{:<12} {:>18} {:>14} {:>12}{}",
                            comp.reference, pos_str, size_str, group_str, fixed_marker
                        );
                    }

                    // Print overlap check
                    println!();
                    if output.overlaps.is_empty() {
                        println!("Overlap check: OK (no overlaps)");
                    } else {
                        println!("Overlap check: FAILED ({} overlaps)", output.overlaps.len());
                        for (a, b) in &output.overlaps {
                            println!("  - {} overlaps with {}", a, b);
                        }
                    }

                    // Print bounding box
                    let (min_x, min_y, max_x, max_y) = output.bounding_box_mils;
                    let width = max_x - min_x;
                    let height = max_y - min_y;
                    println!(
                        "Bounding box: ({}, {}) to ({}, {}) [{} x {} mils]",
                        min_x, min_y, max_x, max_y, width, height
                    );

                    // Print paper bounds
                    println!(
                        "Paper bounds: (0, 0) to ({}, {}) [{} landscape]",
                        output.paper_mils.0, output.paper_mils.1, output.paper_name
                    );

                    // Print legend
                    println!("\n* = position specified in YAML (fixed)");
                }
                Err(e) => {
                    eprintln!("Error computing layout: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Netlist { yaml, filter } => {
            // Parse the YAML file
            let yaml_sch = match YamlSchematic::from_file(&yaml) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing YAML: {}", e);
                    std::process::exit(1);
                }
            };

            let all_connections = yaml_sch.all_connections();

            // Build net map: net_name -> pins
            let mut named_nets: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            let mut anonymous_connections: Vec<Vec<String>> = Vec::new();

            for conn in &all_connections {
                let pins: Vec<String> = conn
                    .pins
                    .iter()
                    .filter(|p| {
                        if let Some(ref f) = filter {
                            p.starts_with(f)
                                && p.chars().nth(f.len()).map(|c| c == ':').unwrap_or(false)
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect();

                if pins.is_empty() && filter.is_some() {
                    continue;
                }

                if let Some(ref net_name) = conn.net {
                    named_nets.entry(net_name.clone()).or_default().extend(pins);
                } else if pins.len() >= 2 {
                    anonymous_connections.push(pins);
                }
            }

            // Print named nets
            let mut net_names: Vec<_> = named_nets.keys().cloned().collect();
            net_names.sort();

            if net_names.is_empty() && anonymous_connections.is_empty() {
                println!("No connections defined");
            } else {
                for name in net_names {
                    if let Some(pins) = named_nets.get(&name) {
                        if !pins.is_empty() {
                            println!("&{}: {}", name, pins.join(", "));
                        }
                    }
                }

                // Print anonymous connections (direct wires)
                for pins in &anonymous_connections {
                    println!("{}", pins.join(" - "));
                }
            }
        }
        Commands::Bom { yaml, no_group_by_value } => {
            // Parse the YAML file
            let yaml_sch = match YamlSchematic::from_file(&yaml) {
                Ok(sch) => sch,
                Err(e) => {
                    eprintln!("Error parsing YAML: {}", e);
                    std::process::exit(1);
                }
            };

            let all_components = yaml_sch.all_components();
            let group_by_value = !no_group_by_value;

            // Group components by symbol (and optionally value)
            // Key: (symbol, value) or (symbol, "") if not grouping by value
            let mut groups: std::collections::HashMap<(String, String), Vec<String>> =
                std::collections::HashMap::new();

            for (reference, component) in &all_components {
                let value = if group_by_value {
                    component.value.clone().unwrap_or_default()
                } else {
                    String::new()
                };
                let key = (component.symbol.clone(), value);
                groups.entry(key).or_default().push(reference.clone());
            }

            // Sort references within each group
            for refs in groups.values_mut() {
                refs.sort_by(|a, b| {
                    // Natural sort: extract prefix and number
                    let parse_ref = |r: &str| -> (String, i32) {
                        let prefix: String = r.chars().take_while(|c| !c.is_ascii_digit()).collect();
                        let num: i32 = r.chars().skip_while(|c| !c.is_ascii_digit())
                            .collect::<String>().parse().unwrap_or(0);
                        (prefix, num)
                    };
                    parse_ref(a).cmp(&parse_ref(b))
                });
            }

            // Sort groups by symbol name, then value
            let mut sorted_groups: Vec<_> = groups.into_iter().collect();
            sorted_groups.sort_by(|a, b| a.0.cmp(&b.0));

            // Print BOM header
            println!("Bill of Materials");
            println!("=================\n");

            if let Some(ref title) = yaml_sch.meta.title {
                println!("Project: {}", title);
            }
            if let Some(ref rev) = yaml_sch.meta.revision {
                println!("Revision: {}", rev);
            }
            if yaml_sch.meta.title.is_some() || yaml_sch.meta.revision.is_some() {
                println!();
            }

            // Print table header
            println!(
                "{:>4}  {:<40} {:<15} {}",
                "Qty", "Description", "Value", "References"
            );
            println!(
                "{:>4}  {:<40} {:<15} {}",
                "---", "-----------", "-----", "----------"
            );

            let mut total_count = 0;
            for ((symbol, value), refs) in &sorted_groups {
                let qty = refs.len();
                total_count += qty;

                // Format references, abbreviating long lists
                let refs_str = if refs.len() <= 5 {
                    refs.join(", ")
                } else {
                    format!("{}, ... ({} total)", refs[..3].join(", "), refs.len())
                };

                let value_str = if value.is_empty() { "-" } else { value.as_str() };

                println!(
                    "{:>4}  {:<40} {:<15} {}",
                    qty, symbol, value_str, refs_str
                );
            }

            println!();
            println!("Total: {} components in {} line items", total_count, sorted_groups.len());
        }
    }
}
