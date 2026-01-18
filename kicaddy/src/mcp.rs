//! MCP (Model Context Protocol) server implementation for kicaddy
//!
//! This module provides an MCP server that exposes kicaddy's schematic editing
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
use serde::Deserialize;
use serde_json::Map;

use libkicaddy::tools::{
    OutlineInput, OutlineTool, Tool,
    UpdateComponentInput, UpdateComponentTool,
    wrappers::{
        PlaceComponentInput, PlaceComponentTool,
        DeleteComponentInput, DeleteComponentTool,
        AddWireInput, AddWireTool,
        DeleteWireInput, DeleteWireTool,
        AddLabelInput, AddLabelTool,
        DeleteLabelInput, DeleteLabelTool,
        SearchSymbolInput, SearchSymbolTool,
        GetConfigInput, GetConfigTool,
        NewSchematicInput, NewSchematicTool,
        ConnectInput, ConnectTool,
        DisconnectInput, DisconnectTool,
        GetNetlistInput, GetNetlistTool,
    },
};

/// The kicaddy MCP service that handles tool calls
#[derive(Debug, Clone, Default)]
pub struct KicaddyService;

impl KicaddyService {
    /// Create a new KicaddyService instance
    pub fn new() -> Self {
        Self
    }
}

// Input request types with schemars support for JSON schema generation

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OutlineRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateComponentRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Reference designator of component to update (e.g., 'R1', 'U1')
    pub reference: String,
    /// New X position in schematic units (optional)
    pub x: Option<f64>,
    /// New Y position in schematic units (optional)
    pub y: Option<f64>,
    /// New rotation angle in degrees (optional)
    pub angle: Option<f64>,
    /// New reference designator to rename component to (optional)
    pub new_reference: Option<String>,
    /// New component value (optional)
    pub value: Option<String>,
    /// Mirror setting: 'x', 'y', or 'none' (optional)
    pub mirror: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlaceComponentRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Library name (e.g., 'Device', 'Connector')
    pub library: String,
    /// Symbol name within the library (e.g., 'R', 'C', 'Conn_01x04')
    pub symbol: String,
    /// X position in schematic units
    pub x: f64,
    /// Y position in schematic units
    pub y: f64,
    /// Rotation angle in degrees (default: 0)
    pub angle: Option<f64>,
    /// Reference designator (e.g., 'R1'). Auto-assigned if not provided
    pub reference: Option<String>,
    /// Component value (e.g., '10k'). Uses symbol name if not provided
    pub value: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteComponentRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Reference designator of component to delete (e.g., 'R1')
    pub reference: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddWireRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Start pin in 'REF:PIN' format (e.g., 'R1:1'). Use either from_pin or from_x/from_y
    pub from_pin: Option<String>,
    /// Start X coordinate. Use either from_pin or from_x/from_y
    pub from_x: Option<f64>,
    /// Start Y coordinate. Use either from_pin or from_x/from_y
    pub from_y: Option<f64>,
    /// End pin in 'REF:PIN' format (e.g., 'U1:VCC'). Use either to_pin or to_x/to_y
    pub to_pin: Option<String>,
    /// End X coordinate. Use either to_pin or to_x/to_y
    pub to_x: Option<f64>,
    /// End Y coordinate. Use either to_pin or to_x/to_y
    pub to_y: Option<f64>,
    /// Routing mode: 'direct' (diagonal), 'orthogonal' (H then V), or 'orthogonal-vh' (V then H). Default: 'direct'
    pub routing: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteWireRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// X coordinate of a point on the wire to delete
    pub x: f64,
    /// Y coordinate of a point on the wire to delete
    pub y: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddLabelRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Label text (net name)
    pub name: String,
    /// Pin in 'REF:PIN' format to attach label to. Use either pin or x/y
    pub pin: Option<String>,
    /// X coordinate. Use either pin or x/y
    pub x: Option<f64>,
    /// Y coordinate. Use either pin or x/y
    pub y: Option<f64>,
    /// Label angle in degrees (auto-detected if pin is used). Default: 0
    pub angle: Option<f64>,
    /// Use global label instead of local. Default: false
    pub global: Option<bool>,
    /// Shape for global label: 'input', 'output', 'bidirectional', 'tri_state', 'passive'. Default: 'input'
    pub shape: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteLabelRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Label text to delete
    pub name: String,
    /// X coordinate (optional, to disambiguate multiple labels with same name)
    pub x: Option<f64>,
    /// Y coordinate (optional, to disambiguate multiple labels with same name)
    pub y: Option<f64>,
}

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
pub struct NewSchematicRequest {
    /// Path for the new schematic file
    pub path: String,
    /// Paper size (default: 'A4'). Options: A4, A3, A2, A1, A0, A, B, C, D, E, USLetter, USLegal, USLedger
    pub paper: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConnectRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Endpoints to connect. Use 'REF:PIN' for pins (e.g., 'R1:1', 'U1:VCC') or '&NET' for net labels (e.g., '&GND', '&VCC')
    pub endpoints: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DisconnectRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Two endpoints to disconnect. Use 'REF:PIN' for pins or '&NET' for net labels
    pub endpoints: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetNetlistRequest {
    /// Path to the KiCAD schematic (.kicad_sch) file
    pub schematic: String,
    /// Optional component reference to filter by (e.g., 'U1'). If not provided, returns all connections.
    pub filter: Option<String>,
}

// Helper function to create a tool definition
fn make_tool<T: JsonSchema>(name: &str, description: &str) -> McpTool {
    let schema = schemars::schema_for!(T);
    let schema_value = serde_json::to_value(&schema).unwrap_or_default();

    // Convert to Arc<Map<String, Value>>
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

impl ServerHandler for KicaddyService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "kicaddy is a tool for manipulating KiCAD schematic files. \
                 Use 'search_symbol' to find components, 'place_component' to add them, \
                 'add_wire' to connect pins, and 'outline' to see the current schematic structure."
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
                    make_tool::<OutlineRequest>(
                        "outline",
                        "Get complete schematic state: all components with pin positions and bounding boxes, nets with connectivity, and paper dimensions. **Use this to understand the schematic before making changes.**",
                    ),
                    make_tool::<UpdateComponentRequest>(
                        "update_component",
                        "Update a component's properties (position, angle, reference, value, mirror)",
                    ),
                    make_tool::<PlaceComponentRequest>(
                        "place_component",
                        "Place a component from a KiCAD symbol library. Position is in mm, snapped to 1.27mm grid. Returns pin positions for immediate wiring. For power symbols, use library='power' (e.g., symbol='GND', 'VCC', '+3V3').",
                    ),
                    make_tool::<DeleteComponentRequest>(
                        "delete_component",
                        "Delete a component and its connected wires/labels from a schematic",
                    ),
                    make_tool::<AddWireRequest>(
                        "add_wire",
                        "Add a wire between two points. Use pin references like 'R1:1' or 'U1:VCC' (preferred) or coordinates. Routing modes: 'direct' (straight line), 'orthogonal' (horizontal then vertical), 'orthogonal-vh' (vertical then horizontal).",
                    ),
                    make_tool::<DeleteWireRequest>(
                        "delete_wire",
                        "Delete a wire from a schematic at the specified coordinates",
                    ),
                    make_tool::<AddLabelRequest>(
                        "add_label",
                        "Add a net label. Local labels connect within a sheet; global labels connect across all sheets. Use pin reference like 'R1:1' or coordinates. For power nets, prefer placing power symbols instead.",
                    ),
                    make_tool::<DeleteLabelRequest>(
                        "delete_label",
                        "Delete a label from a schematic by name",
                    ),
                    make_tool::<SearchSymbolRequest>(
                        "search_symbol",
                        "Search KiCAD symbol libraries by keyword. Examples: 'ESP32', 'resistor 0805', 'USB type C', 'LDO 3.3V'. Returns lib_id (e.g., 'Device:R') for use with place_component.",
                    ),
                    empty_tool(
                        "get_config",
                        "Get detected KiCAD configuration paths",
                    ),
                    make_tool::<NewSchematicRequest>(
                        "new_schematic",
                        "Create a new empty KiCAD schematic file",
                    ),
                    make_tool::<ConnectRequest>(
                        "connect",
                        "Connect pins and nets logically. Examples: connect(['R1:1', 'C1:2']) wires two pins; connect(['U1:1', '&GND']) connects pin to ground; connect(['R1:1', 'C1:2', 'U1:10']) creates star connection with junction.",
                    ),
                    make_tool::<DisconnectRequest>(
                        "disconnect",
                        "Remove connection between two endpoints. Example: disconnect(['R1:1', 'C1:2']) removes wire between two pins.",
                    ),
                    make_tool::<GetNetlistRequest>(
                        "get_netlist",
                        "Get netlist showing pin connections. Returns entries like 'R1:1 - C1:2, U1:10' or 'U1:1 - &GND'. Use filter to show only connections for a specific component.",
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
                "outline" => {
                    let req: OutlineRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = OutlineInput { schematic: PathBuf::from(req.schematic) };
                    match OutlineTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "update_component" => {
                    let req: UpdateComponentRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = UpdateComponentInput {
                        schematic: PathBuf::from(req.schematic),
                        reference: req.reference,
                        x: req.x,
                        y: req.y,
                        angle: req.angle,
                        new_reference: req.new_reference,
                        value: req.value,
                        mirror: req.mirror,
                    };
                    match UpdateComponentTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "place_component" => {
                    let req: PlaceComponentRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = PlaceComponentInput {
                        schematic: PathBuf::from(req.schematic),
                        library: req.library,
                        symbol: req.symbol,
                        x: req.x,
                        y: req.y,
                        angle: req.angle,
                        reference: req.reference,
                        value: req.value,
                    };
                    match PlaceComponentTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "delete_component" => {
                    let req: DeleteComponentRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = DeleteComponentInput {
                        schematic: PathBuf::from(req.schematic),
                        reference: req.reference,
                    };
                    match DeleteComponentTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "add_wire" => {
                    let req: AddWireRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = AddWireInput {
                        schematic: PathBuf::from(req.schematic),
                        from_pin: req.from_pin,
                        from_x: req.from_x,
                        from_y: req.from_y,
                        to_pin: req.to_pin,
                        to_x: req.to_x,
                        to_y: req.to_y,
                        routing: req.routing,
                    };
                    match AddWireTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "delete_wire" => {
                    let req: DeleteWireRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = DeleteWireInput {
                        schematic: PathBuf::from(req.schematic),
                        x: req.x,
                        y: req.y,
                    };
                    match DeleteWireTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "add_label" => {
                    let req: AddLabelRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = AddLabelInput {
                        schematic: PathBuf::from(req.schematic),
                        name: req.name,
                        pin: req.pin,
                        x: req.x,
                        y: req.y,
                        angle: req.angle,
                        global: req.global,
                        shape: req.shape,
                    };
                    match AddLabelTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "delete_label" => {
                    let req: DeleteLabelRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = DeleteLabelInput {
                        schematic: PathBuf::from(req.schematic),
                        name: req.name,
                        x: req.x,
                        y: req.y,
                    };
                    match DeleteLabelTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "search_symbol" => {
                    let req: SearchSymbolRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = SearchSymbolInput {
                        query: req.query,
                        limit: req.limit,
                        library: req.library,
                    };
                    match SearchSymbolTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "get_config" => {
                    let input = GetConfigInput {};
                    match GetConfigTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "new_schematic" => {
                    let req: NewSchematicRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = NewSchematicInput {
                        path: PathBuf::from(req.path),
                        paper: req.paper,
                    };
                    match NewSchematicTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "connect" => {
                    let req: ConnectRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = ConnectInput {
                        schematic: PathBuf::from(req.schematic),
                        endpoints: req.endpoints,
                    };
                    match ConnectTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "disconnect" => {
                    let req: DisconnectRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = DisconnectInput {
                        schematic: PathBuf::from(req.schematic),
                        endpoints: req.endpoints,
                    };
                    match DisconnectTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "get_netlist" => {
                    let req: GetNetlistRequest = serde_json::from_value(args_value)
                        .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid params: {}", e), None))?;
                    let input = GetNetlistInput {
                        schematic: PathBuf::from(req.schematic),
                        filter: req.filter,
                    };
                    match GetNetlistTool::execute(input) {
                        Ok(out) => serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("Error: {}", e)),
                        Err(e) => format!("Error: {}", e),
                    }
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
