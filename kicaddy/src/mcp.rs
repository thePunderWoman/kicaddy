//! MCP (Model Context Protocol) server implementation for kicaddy
//!
//! This module provides an MCP server that exposes kicaddy's YAML schematic
//! functionality as tools that can be called by AI assistants like Claude.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo, Tool as McpTool,
    },
    service::{RequestContext, RoleServer},
    ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Map;

use libkicaddy::tools::{
    wrappers::{SearchSymbolInput, SearchSymbolTool},
    Tool, build_semantic_outline,
};
use libkicaddy::yaml::{YamlSchematic, Compiler};
use libkicaddy::parse_schematic;

/// Example YAML schematic demonstrating all features
const EXAMPLE_YAML: &str = r#"# KiCAD Schematic Definition
# Compile with: kicaddy compile schematic.yaml

# =============================================================================
# METADATA - Optional schematic information
# =============================================================================
meta:
  paper: A4                    # Paper size: A4, A3, A, B, C, D, USLetter, etc.
  title: "Example Circuit"
  author: "kicaddy"
  revision: "1.0"
  date: "2024-01-15"
  company: "ACME Electronics"

# =============================================================================
# GROUPS - Optional logical groupings (purely organizational)
# =============================================================================
groups:
  Power Supply:
    components:
      U1:
        symbol: Regulator_Linear:AP2112K-3.3
        position: [75, 50]
        value: AP2112K-3.3

      C_IN:
        symbol: Device:C
        position: [50, 50]
        value: 10uF

      C_OUT:
        symbol: Device:C
        position: [100, 50]
        value: 10uF

    connections:
      - net: VIN
        pins: [C_IN:1, U1:VIN]
      - net: GND
        pins: [C_IN:2, U1:GND, C_OUT:2]
      - net: 3V3
        pins: [U1:VOUT, C_OUT:1]

# =============================================================================
# COMPONENTS - Top-level component definitions
# =============================================================================
components:
  # Basic component with just required fields
  R1:
    symbol: Device:R           # Format: Library:Symbol
    position: [150, 50]        # [x, y] in mm, snapped to 1.27mm grid
    value: 10k

  # Component with all optional fields
  R2:
    symbol: Device:R
    position: [150, 75]
    angle: 90                  # Rotation: 0, 90, 180, or 270 degrees
    mirror: x                  # Mirror: x or y (optional)
    value: 4.7k
    unit: 1                    # Unit number for multi-unit symbols

  # Alternative position format
  C1:
    symbol: Device:C
    position:
      x: 175
      y: 50
    value: 100nF

  # LED example
  D1:
    symbol: Device:LED
    position: [200, 50]
    value: RED

# =============================================================================
# CONNECTIONS - Wire and net definitions
# =============================================================================
connections:
  # Direct wire between two pins (no net name)
  - pins: [R1:2, R2:1]

  # Named net connecting multiple pins
  - net: 3V3
    pins: [R1:1, C1:1]

  # Power nets (GND, VCC, etc.) automatically use global labels
  - net: GND
    pins: [R2:2, C1:2, D1:K]

  # Single pin to named net (creates a label)
  - net: LED_ANODE
    pins: [D1:A]

  # Explicit global label (for non-power nets that span sheets)
  - net: SIGNAL_OUT
    pins: [R1:2]
    global: true

# =============================================================================
# PIN REFERENCE FORMATS
# =============================================================================
# Pins are referenced as: REFERENCE:PIN
#
# PIN can be:
#   - Pin number: R1:1, R1:2, U1:4
#   - Pin name: U1:VCC, U1:GND, U1:OUT (if symbol defines named pins)
#
# Use search_symbol tool to find available pins for a symbol.
"#;

/// The kicaddy MCP service that handles tool calls
#[derive(Debug, Clone, Default)]
pub struct KicaddyService;

impl KicaddyService {
    /// Create a new KicaddyService instance
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// Request Types
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolRequest {
    /// Search query (e.g., 'current sensor i2c', 'resistor', 'op amp')
    pub query: String,
    /// Maximum number of results (default: 10)
    pub limit: Option<usize>,
    /// Filter to a specific library (optional)
    pub library: Option<String>,
}


#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompileYamlRequest {
    /// Path to the YAML schematic definition file
    pub yaml_path: String,
    /// Output path for the .kicad_sch file (optional, defaults to same name with .kicad_sch extension)
    pub output_path: Option<String>,
    /// Only validate the YAML file without compiling
    pub validate_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OutlineRequest {
    /// Path to .kicad_sch file
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NetlistRequest {
    /// Path to the YAML schematic definition file
    pub yaml_path: String,
    /// Optional component reference to filter by (e.g., 'U1')
    pub filter: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSymbolInfoRequest {
    /// Library name (e.g., 'Device', 'Connector', 'Espressif')
    pub library: String,
    /// Symbol name within the library (e.g., 'R', 'C', 'ESP32-C6-WROOM-1')
    pub symbol: String,
}

// ============================================================================
// Output Types for tools
// ============================================================================

#[derive(Debug, Serialize)]
struct SymbolInfoOutput {
    /// Full lib_id (e.g., "Device:R")
    lib_id: String,
    /// Symbol name
    name: String,
    /// Library name
    library: String,
    /// Description (from Description property)
    description: Option<String>,
    /// Datasheet URL (from Datasheet property)
    datasheet: Option<String>,
    /// Default footprint (from Footprint property)
    footprint: Option<String>,
    /// Keywords for searching
    keywords: Option<String>,
    /// Reference designator prefix (e.g., "R", "C", "U")
    reference_prefix: Option<String>,
    /// Default value
    default_value: Option<String>,
    /// Number of units (for multi-unit symbols)
    unit_count: usize,
    /// List of pins
    pins: Vec<PinInfoOutput>,
    /// Include in BOM
    in_bom: bool,
    /// Include on board
    on_board: bool,
}

#[derive(Debug, Serialize)]
struct PinInfoOutput {
    /// Pin number (what you use in connections, e.g., "1", "2", "VCC")
    number: String,
    /// Pin name (descriptive, e.g., "~", "VCC", "GND")
    name: String,
    /// Electrical type (input, output, passive, power_in, etc.)
    electrical_type: String,
    /// Whether the pin is hidden
    hidden: bool,
}


// ============================================================================
// Helper Functions
// ============================================================================

fn make_tool<T: JsonSchema>(name: &str, description: &str) -> McpTool {
    let schema = schemars::schema_for!(T);
    let schema_value = serde_json::to_value(&schema).unwrap_or_default();

    let input_schema = if let serde_json::Value::Object(map) = schema_value {
        Arc::new(map)
    } else {
        Arc::new(Map::new())
    };

    McpTool {
        name: Cow::Owned(name.to_string()),
        description: Some(Cow::Owned(description.to_string())),
        input_schema,
        annotations: None,
        icons: None,
        output_schema: None,
        title: None,
    }
}

fn empty_tool(name: &str, description: &str) -> McpTool {
    let mut map = Map::new();
    map.insert("type".to_string(), serde_json::Value::String("object".to_string()));
    map.insert("properties".to_string(), serde_json::Value::Object(Map::new()));

    McpTool {
        name: Cow::Owned(name.to_string()),
        description: Some(Cow::Owned(description.to_string())),
        input_schema: Arc::new(map),
        annotations: None,
        icons: None,
        output_schema: None,
        title: None,
    }
}

// ============================================================================
// Tool Implementations
// ============================================================================

fn execute_outline(file_path: &str) -> Result<String, String> {
    let path = PathBuf::from(file_path);

    let schematic = parse_schematic(&path)
        .map_err(|e| format!("Failed to parse schematic: {}", e))?;

    let outline = build_semantic_outline(&schematic);
    Ok(outline.to_text())
}

fn execute_netlist(yaml_path: &str, filter: Option<&str>) -> Result<String, String> {
    let path = PathBuf::from(yaml_path);
    let yaml_sch = YamlSchematic::from_file(&path)
        .map_err(|e| format!("Failed to parse YAML: {}", e))?;

    let all_connections = yaml_sch.all_connections();

    // Build net map: net_name -> pins
    let mut named_nets: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut anonymous_connections: Vec<Vec<String>> = Vec::new();

    for conn in &all_connections {
        let pins: Vec<String> = conn.pins.iter()
            .filter(|p| {
                if let Some(f) = filter {
                    p.starts_with(f) && p.chars().nth(f.len()).map(|c| c == ':').unwrap_or(false)
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
            named_nets.entry(net_name.clone())
                .or_default()
                .extend(pins);
        } else if pins.len() >= 2 {
            anonymous_connections.push(pins);
        }
    }

    // Build output lines
    let mut lines: Vec<String> = Vec::new();

    // Named nets
    let mut net_names: Vec<_> = named_nets.keys().cloned().collect();
    net_names.sort();
    for name in net_names {
        if let Some(pins) = named_nets.get(&name) {
            if !pins.is_empty() {
                lines.push(format!("&{}: {}", name, pins.join(", ")));
            }
        }
    }

    // Anonymous connections (direct wires)
    for pins in &anonymous_connections {
        lines.push(pins.join(" - "));
    }

    if lines.is_empty() {
        Ok("No connections defined".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

fn execute_compile_yaml(
    yaml_path: &str,
    output_path: Option<&str>,
    validate_only: bool,
) -> Result<String, String> {
    let path = PathBuf::from(yaml_path);
    let yaml_sch = YamlSchematic::from_file(&path)
        .map_err(|e| format!("Failed to parse YAML: {}", e))?;

    let compiler = Compiler::new()
        .map_err(|e| format!("KiCAD config error: {}", e))?;

    if validate_only {
        let result = compiler.validate(&yaml_sch);
        let is_valid = result.is_valid();
        let output = serde_json::json!({
            "is_valid": is_valid,
            "errors": result.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            "warnings": result.warnings,
        });
        return serde_json::to_string_pretty(&output).map_err(|e| e.to_string());
    }

    let out_path = output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| path.with_extension("kicad_sch"));
    // Real KiCad projects derive their `instances` block project name from the
    // .kicad_pro/root schematic file stem; kicaddy has no .kicad_pro of its own, so the
    // output filename is the closest equivalent available here.
    let compiler = match out_path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => compiler.with_project_name(stem),
        None => compiler,
    };

    match compiler.compile(&yaml_sch) {
        Ok(compile_output) => {
            compile_output.root.write_to_file(&out_path)
                .map_err(|e| format!("Failed to write schematic: {}", e))?;

            for (sheet_name, child_schematic) in &compile_output.children {
                let sheet_file = yaml_sch
                    .sheets
                    .get(sheet_name)
                    .and_then(|sheet| sheet.path.clone())
                    .unwrap_or_else(|| sheet_name.clone());
                let child_path = out_path.with_file_name(if sheet_file.ends_with(".kicad_sch") {
                    sheet_file
                } else {
                    format!("{}.kicad_sch", sheet_file)
                });
                child_schematic
                    .write_to_file(&child_path)
                    .map_err(|e| format!("Failed to write child sheet '{}': {}", sheet_name, e))?;
            }

            let output = serde_json::json!({
                "success": true,
                "output_path": out_path.to_string_lossy(),
                "components_placed": compile_output.components_placed.len(),
                "connections_made": compile_output.connections_made,
                "warnings": compile_output.warnings,
            });
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())
        }
        Err(e) => {
            let output = serde_json::json!({
                "success": false,
                "error": e.to_string(),
            });
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())
        }
    }
}

fn execute_get_config() -> Result<String, String> {
    use libkicaddy::KicadConfig;

    let config = KicadConfig::detect()
        .map_err(|e| format!("Config error: {}", e))?;

    let output = serde_json::json!({
        "kicad_path": config.kicad_path.to_string_lossy(),
        "symbol_lib_path": config.symbol_lib_path.to_string_lossy(),
        "footprint_lib_path": config.footprint_lib_path.to_string_lossy(),
    });
    serde_json::to_string_pretty(&output).map_err(|e| e.to_string())
}

fn execute_search_symbol(query: &str, limit: Option<usize>, library: Option<&str>) -> Result<String, String> {
    let input = SearchSymbolInput {
        query: query.to_string(),
        limit,
        library: library.map(String::from),
    };

    match SearchSymbolTool::execute(input) {
        Ok(output) => serde_json::to_string_pretty(&output).map_err(|e| e.to_string()),
        Err(e) => Err(format!("Search error: {}", e)),
    }
}

fn execute_get_symbol_info(library: &str, symbol: &str) -> Result<String, String> {
    use libkicaddy::symbol::lookup::find_symbol;
    use libkicaddy::KicadConfig;

    let config = KicadConfig::detect()
        .map_err(|e| format!("Config error: {}", e))?;

    let sym = find_symbol(&config, library, symbol)
        .map_err(|e| format!("{}", e))?;

    // Collect all pins from all units, deduplicating by pin number
    let mut seen_pins = std::collections::HashSet::new();
    let mut pins: Vec<PinInfoOutput> = Vec::new();

    for pin in sym.pins() {
        if seen_pins.insert(pin.number.number.clone()) {
            pins.push(PinInfoOutput {
                number: pin.number.number.clone(),
                name: pin.name.name.clone(),
                electrical_type: pin.electrical_type.as_str().to_string(),
                hidden: pin.hide,
            });
        }
    }

    // Sort pins: numeric pins first (sorted numerically), then alphanumeric
    pins.sort_by(|a, b| {
        let a_num: Option<i32> = a.number.parse().ok();
        let b_num: Option<i32> = b.number.parse().ok();
        match (a_num, b_num) {
            (Some(an), Some(bn)) => an.cmp(&bn),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.number.cmp(&b.number),
        }
    });

    // Count unique units (excluding style variants like _0_1, _1_1)
    let unit_count = sym.units.iter()
        .filter(|u| u.name.ends_with("_1"))
        .count()
        .max(1);

    // Get datasheet URL, filtering out placeholder values
    let datasheet = sym.property("Datasheet")
        .map(|p| p.value.as_str())
        .filter(|v| !v.is_empty() && *v != "~" && !v.starts_with("${"))
        .map(|s| s.to_string());

    let output = SymbolInfoOutput {
        lib_id: format!("{}:{}", library, symbol),
        name: sym.name.clone(),
        library: library.to_string(),
        description: sym.description().map(|s| s.to_string()),
        datasheet,
        footprint: sym.footprint()
            .filter(|v| !v.is_empty() && *v != "~")
            .map(|s| s.to_string()),
        keywords: sym.keywords().map(|s| s.to_string()),
        reference_prefix: sym.reference().map(|s| s.to_string()),
        default_value: sym.value().map(|s| s.to_string()),
        unit_count,
        pins,
        in_bom: sym.in_bom,
        on_board: sym.on_board,
    };

    serde_json::to_string_pretty(&output).map_err(|e| e.to_string())
}

// ============================================================================
// MCP Server Handler
// ============================================================================

impl ServerHandler for KicaddyService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "kicaddy is a tool for creating KiCAD schematics from YAML definitions. \
                 Use 'search_symbol' to find components, write a YAML schematic file, \
                 use 'outline' to review it, and 'compile_yaml' to generate the KiCAD file."
                    .to_string(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            Ok(ListToolsResult {
                tools: vec![
                    make_tool::<SearchSymbolRequest>(
                        "search_symbol",
                        "Search KiCAD symbol libraries by keyword. Returns lib_id (e.g., 'Device:R') and basic info for use in YAML schematics.",
                    ),
                    make_tool::<GetSymbolInfoRequest>(
                        "get_symbol_info",
                        "Get detailed information about a specific symbol including all pins, datasheet URL, footprint, and description.",
                    ),
                    empty_tool(
                        "get_config",
                        "Get detected KiCAD configuration paths.",
                    ),
                    make_tool::<OutlineRequest>(
                        "outline",
                        "Get a semantic outline of a .kicad_sch schematic. Shows components with pins and connections with automatic detection of pullup/pulldown resistors and decoupling caps.",
                    ),
                    make_tool::<NetlistRequest>(
                        "netlist",
                        "Get the netlist from a YAML schematic showing which pins are connected to which nets.",
                    ),
                    make_tool::<CompileYamlRequest>(
                        "compile_yaml",
                        "Compile a YAML schematic definition to KiCAD format. Use validate_only=true to check for errors without generating output.",
                    ),
                    empty_tool(
                        "example_yaml",
                        "Output an example YAML schematic showing all supported features.",
                    ),
                ],
                next_cursor: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            let name = request.name.as_ref();
            let args_value = serde_json::Value::Object(
                request.arguments.map(|a| a.clone()).unwrap_or_default()
            );

            let result = match name {
                "search_symbol" => {
                    let req: SearchSymbolRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    execute_search_symbol(&req.query, req.limit, req.library.as_deref())
                        .unwrap_or_else(|e| format!("Error: {}", e))
                }
                "get_symbol_info" => {
                    let req: GetSymbolInfoRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    execute_get_symbol_info(&req.library, &req.symbol)
                        .unwrap_or_else(|e| format!("Error: {}", e))
                }
                "get_config" => {
                    execute_get_config()
                        .unwrap_or_else(|e| format!("Error: {}", e))
                }
                "outline" => {
                    let req: OutlineRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    execute_outline(&req.path)
                        .unwrap_or_else(|e| format!("Error: {}", e))
                }
                "netlist" => {
                    let req: NetlistRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    execute_netlist(&req.yaml_path, req.filter.as_deref())
                        .unwrap_or_else(|e| format!("Error: {}", e))
                }
                "compile_yaml" => {
                    let req: CompileYamlRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    execute_compile_yaml(
                        &req.yaml_path,
                        req.output_path.as_deref(),
                        req.validate_only.unwrap_or(false),
                    ).unwrap_or_else(|e| format!("Error: {}", e))
                }
                "example_yaml" => {
                    EXAMPLE_YAML.to_string()
                }
                _ => {
                    return Err(rmcp::ErrorData::invalid_request(
                        format!("Unknown tool: {}", name),
                        None,
                    ));
                }
            };

            Ok(CallToolResult::success(vec![Content::text(result)]))
        }
    }
}

/// Run the MCP server on stdio
pub async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::{transport::stdio, ServiceExt};

    let service = KicaddyService::new();
    let server = service.serve(stdio()).await?;
    server.waiting().await?;

    Ok(())
}
