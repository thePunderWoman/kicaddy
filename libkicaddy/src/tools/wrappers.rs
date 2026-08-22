//! Wrappers around existing commands as stateless tools
//!
//! Each tool takes a schematic path and reads/writes the file per call.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::outline::BoundingBox;
use super::{load_schematic, save_schematic, Tool, ToolError};
use crate::commands::{
    parse_label_shape, parse_pin_ref, parse_routing_mode, AddLabelCommand, AddWireCommand,
    Command, DeleteComponentCommand, DeleteLabelCommand, DeleteWireCommand, LabelLocation,
    PlaceComponentCommand, WireEndpoint,
};
use crate::common::{Point, Position};
use crate::schematic::{Mirror, PaperSize, Schematic, SymbolInstance};
use crate::symbol::graphics::GraphicItem;
use crate::symbol::Symbol;
use crate::{find_symbol, search, KicadConfig, SearchOptions};

// ============================================================================
// PlaceComponent Tool
// ============================================================================

/// Input for place_component tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlaceComponentInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Library name (e.g., "Device")
    pub library: String,

    /// Symbol name within the library (e.g., "R")
    pub symbol: String,

    /// X position in schematic units
    pub x: f64,

    /// Y position in schematic units
    pub y: f64,

    /// Rotation angle in degrees (default: 0)
    #[serde(default)]
    pub angle: Option<f64>,

    /// Reference designator (e.g., "R1"). Auto-assigned if not provided.
    #[serde(default)]
    pub reference: Option<String>,

    /// Component value. Uses symbol name if not provided.
    #[serde(default)]
    pub value: Option<String>,
}

/// Pin position info for placed component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedPinInfo {
    /// Pin number
    pub number: String,
    /// Pin name
    pub name: String,
    /// Pin X position in schematic coordinates
    pub x: f64,
    /// Pin Y position in schematic coordinates
    pub y: f64,
}

/// Output of place_component tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceComponentOutput {
    /// Reference designator assigned
    pub reference: String,
    /// Final X position (may be snapped)
    pub x: f64,
    /// Final Y position (may be snapped)
    pub y: f64,
    /// Library ID (e.g., "Device:R")
    pub lib_id: String,
    /// Whether position was snapped to grid
    pub was_snapped: bool,
    /// Pin positions for immediate wiring
    pub pins: Vec<PlacedPinInfo>,
    /// Component bounding box
    pub bounds: BoundingBox,
}

pub struct PlaceComponentTool;

impl Tool for PlaceComponentTool {
    const NAME: &'static str = "place_component";
    const DESCRIPTION: &'static str = "Place a component from a KiCAD symbol library. Position is in mm, snapped to 1.27mm grid. Returns pin positions for immediate wiring. For power symbols, use library='power' (e.g., symbol='GND', 'VCC', '+3V3').";

    type Input = PlaceComponentInput;
    type Output = PlaceComponentOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let config = KicadConfig::detect()?;
        let sym = find_symbol(&config, &input.library, &input.symbol)?;
        let mut schematic = load_schematic(&input.schematic)?;

        let lib_id = format!("{}:{}", input.library, input.symbol);
        let cmd = PlaceComponentCommand {
            symbol: sym.clone(),
            lib_id: lib_id.clone(),
            position: Position::new(input.x, input.y, input.angle.unwrap_or(0.0)),
            reference: input.reference,
            value: input.value,
        };

        let result = cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        // Find the placed symbol to get pin positions and bounds
        let placed_symbol = schematic.find_symbol_by_reference(&result.reference);

        let (pins, bounds) = if let Some(symbol) = placed_symbol {
            let lib_symbol = schematic.lib_symbols.iter().find(|s| s.name == lib_id);

            // Get pin positions
            let pins: Vec<PlacedPinInfo> = symbol.pins.iter().filter_map(|pin| {
                let (pos, _angle) = schematic.get_pin_position(symbol, &pin.number)?;
                let pin_name = lib_symbol
                    .and_then(|s| {
                        s.units.iter()
                            .flat_map(|u| u.pins.iter())
                            .find(|p| p.number.number == pin.number)
                            .map(|p| p.name.name.clone())
                    })
                    .unwrap_or_else(|| pin.number.clone());

                Some(PlacedPinInfo {
                    number: pin.number.clone(),
                    name: pin_name,
                    x: pos.x,
                    y: pos.y,
                })
            }).collect();

            // Calculate bounds
            let bounds = calculate_component_bounds(symbol, lib_symbol, &schematic);

            (pins, bounds)
        } else {
            (Vec::new(), BoundingBox { min_x: result.x, min_y: result.y, max_x: result.x, max_y: result.y })
        };

        Ok(PlaceComponentOutput {
            reference: result.reference,
            x: result.x,
            y: result.y,
            lib_id,
            was_snapped: result.was_snapped,
            pins,
            bounds,
        })
    }
}

// ============================================================================
// DeleteComponent Tool
// ============================================================================

/// Input for delete_component tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteComponentInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Reference designator of component to delete (e.g., "R1")
    pub reference: String,
}

/// Output of delete_component tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteComponentOutput {
    /// Reference of deleted component
    pub reference: String,
    /// Whether deletion was successful
    pub success: bool,
}

pub struct DeleteComponentTool;

impl Tool for DeleteComponentTool {
    const NAME: &'static str = "delete_component";
    const DESCRIPTION: &'static str =
        "Delete a component from a schematic (also removes connected wires and labels at pins)";

    type Input = DeleteComponentInput;
    type Output = DeleteComponentOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut schematic = load_schematic(&input.schematic)?;

        let cmd = DeleteComponentCommand {
            reference: input.reference.clone(),
        };

        cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        Ok(DeleteComponentOutput {
            reference: input.reference,
            success: true,
        })
    }
}

// ============================================================================
// AddWire Tool
// ============================================================================

/// Input for add_wire tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddWireInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Start pin in "REF:PIN" format (e.g., "R1:1"). Mutually exclusive with from_x/from_y.
    #[serde(default)]
    pub from_pin: Option<String>,

    /// Start X coordinate (alternative to from_pin)
    #[serde(default)]
    pub from_x: Option<f64>,

    /// Start Y coordinate (alternative to from_pin)
    #[serde(default)]
    pub from_y: Option<f64>,

    /// End pin in "REF:PIN" format (e.g., "U1:VCC"). Mutually exclusive with to_x/to_y.
    #[serde(default)]
    pub to_pin: Option<String>,

    /// End X coordinate (alternative to to_pin)
    #[serde(default)]
    pub to_x: Option<f64>,

    /// End Y coordinate (alternative to to_pin)
    #[serde(default)]
    pub to_y: Option<f64>,

    /// Routing mode: "direct", "orthogonal", or "orthogonal-vh" (default: "direct")
    #[serde(default)]
    pub routing: Option<String>,
}

/// Output of add_wire tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddWireOutput {
    /// Start position
    pub from_x: f64,
    pub from_y: f64,
    /// End position
    pub to_x: f64,
    pub to_y: f64,
    /// Routing mode used
    pub routing: String,
}

pub struct AddWireTool;

impl Tool for AddWireTool {
    const NAME: &'static str = "add_wire";
    const DESCRIPTION: &'static str = "Add a wire between two points. Use pin references like 'R1:1' or 'U1:VCC' (preferred) or coordinates. Routing modes: 'direct' (straight line), 'orthogonal' (horizontal then vertical), 'orthogonal-vh' (vertical then horizontal).";

    type Input = AddWireInput;
    type Output = AddWireOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut schematic = load_schematic(&input.schematic)?;

        let routing_str = input.routing.as_deref().unwrap_or("direct");
        let mode = parse_routing_mode(routing_str)?;

        // Parse from endpoint
        let from_endpoint = match (&input.from_pin, input.from_x, input.from_y) {
            (Some(pin_ref), _, _) => {
                let (r, p) = parse_pin_ref(pin_ref)?;
                WireEndpoint::Pin {
                    reference: r,
                    pin: p,
                }
            }
            (None, Some(x), Some(y)) => WireEndpoint::Point(Point::new(x, y)),
            _ => {
                return Err(ToolError::InvalidInput(
                    "Must specify either from_pin or from_x/from_y".to_string(),
                ))
            }
        };

        // Parse to endpoint
        let to_endpoint = match (&input.to_pin, input.to_x, input.to_y) {
            (Some(pin_ref), _, _) => {
                let (r, p) = parse_pin_ref(pin_ref)?;
                WireEndpoint::Pin {
                    reference: r,
                    pin: p,
                }
            }
            (None, Some(x), Some(y)) => WireEndpoint::Point(Point::new(x, y)),
            _ => {
                return Err(ToolError::InvalidInput(
                    "Must specify either to_pin or to_x/to_y".to_string(),
                ))
            }
        };

        let cmd = AddWireCommand {
            from: from_endpoint,
            to: to_endpoint,
            routing: mode,
        };

        let result = cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        Ok(AddWireOutput {
            from_x: result.from.x,
            from_y: result.from.y,
            to_x: result.to.x,
            to_y: result.to.y,
            routing: routing_str.to_string(),
        })
    }
}

// ============================================================================
// DeleteWire Tool
// ============================================================================

/// Input for delete_wire tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteWireInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// X coordinate of a point on the wire
    pub x: f64,

    /// Y coordinate of a point on the wire
    pub y: f64,
}

/// Output of delete_wire tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteWireOutput {
    /// Position where wire was deleted
    pub x: f64,
    pub y: f64,
    /// Whether deletion was successful
    pub success: bool,
}

pub struct DeleteWireTool;

impl Tool for DeleteWireTool {
    const NAME: &'static str = "delete_wire";
    const DESCRIPTION: &'static str = "Delete a wire from a schematic at a given point";

    type Input = DeleteWireInput;
    type Output = DeleteWireOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut schematic = load_schematic(&input.schematic)?;

        let cmd = DeleteWireCommand {
            point: Point::new(input.x, input.y),
        };

        cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        Ok(DeleteWireOutput {
            x: input.x,
            y: input.y,
            success: true,
        })
    }
}

// ============================================================================
// AddLabel Tool
// ============================================================================

/// Input for add_label tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddLabelInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Label text (net name)
    pub name: String,

    /// Pin in "REF:PIN" format (e.g., "R1:1"). Mutually exclusive with x/y.
    #[serde(default)]
    pub pin: Option<String>,

    /// X coordinate (alternative to pin)
    #[serde(default)]
    pub x: Option<f64>,

    /// Y coordinate (alternative to pin)
    #[serde(default)]
    pub y: Option<f64>,

    /// Label angle in degrees (auto-detected if pin is used, default: 0)
    #[serde(default)]
    pub angle: Option<f64>,

    /// Use global label instead of local (default: false)
    #[serde(default)]
    pub global: Option<bool>,

    /// Shape for global label: "input", "output", "bidirectional", "tri_state", "passive" (default: "input")
    #[serde(default)]
    pub shape: Option<String>,
}

/// Output of add_label tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLabelOutput {
    /// Label text
    pub text: String,
    /// Label position
    pub x: f64,
    pub y: f64,
    /// Whether global label
    pub global: bool,
}

pub struct AddLabelTool;

impl Tool for AddLabelTool {
    const NAME: &'static str = "add_label";
    const DESCRIPTION: &'static str = "Add a net label. Local labels connect within a sheet; global labels connect across all sheets. Use pin reference like 'R1:1' or coordinates. For power nets, prefer placing power symbols instead.";

    type Input = AddLabelInput;
    type Output = AddLabelOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut schematic = load_schematic(&input.schematic)?;

        let shape_str = input.shape.as_deref().unwrap_or("input");
        let label_shape = parse_label_shape(shape_str)?;

        // Determine location
        let location = match (&input.pin, input.x, input.y) {
            (Some(pin_ref), _, _) => {
                let (reference, pin) = parse_pin_ref(pin_ref)?;
                LabelLocation::Pin { reference, pin }
            }
            (None, Some(x), Some(y)) => {
                LabelLocation::Position(Position::new(x, y, input.angle.unwrap_or(0.0)))
            }
            _ => {
                return Err(ToolError::InvalidInput(
                    "Must specify either pin or x/y coordinates".to_string(),
                ))
            }
        };

        let cmd = AddLabelCommand {
            text: input.name.clone(),
            location,
            global: input.global.unwrap_or(false),
            shape: label_shape,
        };

        let result = cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        Ok(AddLabelOutput {
            text: result.text,
            x: result.position.x,
            y: result.position.y,
            global: result.global,
        })
    }
}

// ============================================================================
// DeleteLabel Tool
// ============================================================================

/// Input for delete_label tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteLabelInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Label text to delete
    pub name: String,

    /// X coordinate (optional, to disambiguate multiple labels with same name)
    #[serde(default)]
    pub x: Option<f64>,

    /// Y coordinate (optional, to disambiguate multiple labels with same name)
    #[serde(default)]
    pub y: Option<f64>,
}

/// Output of delete_label tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteLabelOutput {
    /// Deleted label text
    pub name: String,
    /// Whether deletion was successful
    pub success: bool,
}

pub struct DeleteLabelTool;

impl Tool for DeleteLabelTool {
    const NAME: &'static str = "delete_label";
    const DESCRIPTION: &'static str = "Delete a label from a schematic by name";

    type Input = DeleteLabelInput;
    type Output = DeleteLabelOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut schematic = load_schematic(&input.schematic)?;

        let position = match (input.x, input.y) {
            (Some(x), Some(y)) => Some(Point::new(x, y)),
            _ => None,
        };

        let cmd = DeleteLabelCommand {
            name: input.name.clone(),
            position,
        };

        cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        Ok(DeleteLabelOutput {
            name: input.name,
            success: true,
        })
    }
}

// ============================================================================
// SearchSymbol Tool
// ============================================================================

/// Input for search_symbol tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchSymbolInput {
    /// Search query (e.g., "current sensor i2c")
    pub query: String,

    /// Maximum number of results (default: 10)
    #[serde(default)]
    pub limit: Option<usize>,

    /// Filter to a specific library
    #[serde(default)]
    pub library: Option<String>,
}

/// Output of search_symbol tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSymbolOutput {
    /// Search results
    pub results: Vec<SymbolSearchResult>,
    /// Total number of hits
    pub total_hits: usize,
    /// Query time in milliseconds
    pub query_time_ms: u64,
}

/// A single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSearchResult {
    /// Full library ID (e.g., "Device:R")
    pub lib_id: String,
    /// Reference designator prefix (e.g., "R")
    pub reference: String,
    /// Symbol description
    pub description: String,
    /// Keywords
    pub keywords: String,
    /// Search score
    pub score: f32,
}

pub struct SearchSymbolTool;

impl Tool for SearchSymbolTool {
    const NAME: &'static str = "search_symbol";
    const DESCRIPTION: &'static str = "Search KiCAD symbol libraries by keyword. Examples: 'ESP32', 'resistor 0805', 'USB type C', 'LDO 3.3V'. Returns lib_id (e.g., 'Device:R') for use with place_component.";

    type Input = SearchSymbolInput;
    type Output = SearchSymbolOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut options = SearchOptions::new().with_limit(input.limit.unwrap_or(10));
        if let Some(lib) = input.library {
            options = options.with_library_filter(lib);
        }

        let results = search(&input.query, options)
            .map_err(|e| ToolError::Other(format!("Search error: {}", e)))?;

        Ok(SearchSymbolOutput {
            results: results
                .results
                .into_iter()
                .map(|r| SymbolSearchResult {
                    lib_id: r.lib_id,
                    reference: r.reference,
                    description: r.description,
                    keywords: r.keywords,
                    score: r.score,
                })
                .collect(),
            total_hits: results.total_hits,
            query_time_ms: results.query_time_ms,
        })
    }
}

// ============================================================================
// GetConfig Tool
// ============================================================================

/// Input for get_config tool (no parameters needed)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetConfigInput {}

/// Output of get_config tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetConfigOutput {
    /// KiCAD installation path
    pub kicad_path: String,
    /// Symbol libraries path
    pub symbol_lib_path: String,
    /// Footprint libraries path
    pub footprint_lib_path: String,
}

pub struct GetConfigTool;

impl Tool for GetConfigTool {
    const NAME: &'static str = "get_config";
    const DESCRIPTION: &'static str = "Get detected KiCAD configuration paths";

    type Input = GetConfigInput;
    type Output = GetConfigOutput;

    fn execute(_input: Self::Input) -> Result<Self::Output, ToolError> {
        let config = KicadConfig::detect()?;

        Ok(GetConfigOutput {
            kicad_path: config.kicad_path.to_string_lossy().to_string(),
            symbol_lib_path: config.symbol_lib_path.to_string_lossy().to_string(),
            footprint_lib_path: config.footprint_lib_path.to_string_lossy().to_string(),
        })
    }
}

// ============================================================================
// NewSchematic Tool
// ============================================================================

/// Input for new_schematic tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NewSchematicInput {
    /// Path for the new schematic file
    pub path: PathBuf,

    /// Paper size (default: "A4"). Options: A4, A3, A2, A1, A0, A, B, C, D, E, USLetter, USLegal, USLedger
    #[serde(default)]
    pub paper: Option<String>,
}

/// Output of new_schematic tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSchematicOutput {
    /// Path of created schematic
    pub path: String,
    /// Paper size used
    pub paper: String,
}

pub struct NewSchematicTool;

impl Tool for NewSchematicTool {
    const NAME: &'static str = "new_schematic";
    const DESCRIPTION: &'static str = "Create a new empty KiCAD schematic file";

    type Input = NewSchematicInput;
    type Output = NewSchematicOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        let mut schematic = Schematic::new();

        // Set paper size if specified
        let paper_str = input.paper.as_deref().unwrap_or("A4");
        schematic.paper = PaperSize::from_str(paper_str);

        // Write the schematic
        schematic.write_to_file(&input.path)
            .map_err(|e| ToolError::WriteError(e.to_string()))?;

        Ok(NewSchematicOutput {
            path: input.path.to_string_lossy().to_string(),
            paper: schematic.paper.as_str().to_string(),
        })
    }
}

// ============================================================================
// Connect Tool
// ============================================================================

/// Input for connect tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConnectInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Endpoints to connect. Format: "REF:PIN" for pins (e.g., "R1:1", "U1:VCC")
    /// or "&NET" for net labels (e.g., "&GND", "&VCC", "&SDA").
    /// Minimum 2 endpoints required.
    pub endpoints: Vec<String>,
}

/// Output of connect tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectOutput {
    /// Number of wires created
    pub wires_created: usize,
    /// Number of junctions created
    pub junctions_created: usize,
    /// Labels created (if connecting to a net)
    pub labels_created: Vec<String>,
    /// Net name if connecting to a net
    pub net_name: Option<String>,
}

pub struct ConnectTool;

impl Tool for ConnectTool {
    const NAME: &'static str = "connect";
    const DESCRIPTION: &'static str = "Connect pins and nets logically. Examples: connect(['R1:1', 'C1:2']) wires two pins; connect(['U1:1', '&GND']) connects pin to ground; connect(['R1:1', 'C1:2', 'U1:10']) creates star connection with junction.";

    type Input = ConnectInput;
    type Output = ConnectOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        use crate::commands::{ConnectCommand, Command};

        let mut schematic = load_schematic(&input.schematic)?;

        let cmd = ConnectCommand {
            endpoints: input.endpoints,
        };

        let result = cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        Ok(ConnectOutput {
            wires_created: result.wires_created,
            junctions_created: result.junctions_created,
            labels_created: result.labels_created,
            net_name: result.net_name,
        })
    }
}

// ============================================================================
// Disconnect Tool
// ============================================================================

/// Input for disconnect tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DisconnectInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Endpoints to disconnect (exactly 2). Format: "REF:PIN" for pins or "&NET" for net labels.
    pub endpoints: Vec<String>,
}

/// Output of disconnect tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectOutput {
    /// Number of wires removed
    pub wires_removed: usize,
    /// Number of junctions removed
    pub junctions_removed: usize,
    /// Number of labels removed
    pub labels_removed: usize,
}

pub struct DisconnectTool;

impl Tool for DisconnectTool {
    const NAME: &'static str = "disconnect";
    const DESCRIPTION: &'static str = "Remove connection between two endpoints. Example: disconnect(['R1:1', 'C1:2']) removes wire between two pins.";

    type Input = DisconnectInput;
    type Output = DisconnectOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        use crate::commands::{DisconnectCommand, Command};

        if input.endpoints.len() != 2 {
            return Err(ToolError::InvalidInput(
                "Exactly 2 endpoints are required for disconnect".to_string(),
            ));
        }

        let mut schematic = load_schematic(&input.schematic)?;

        let cmd = DisconnectCommand {
            endpoint1: input.endpoints[0].clone(),
            endpoint2: input.endpoints[1].clone(),
        };

        let result = cmd.execute(&mut schematic)?;
        save_schematic(&schematic, &input.schematic)?;

        Ok(DisconnectOutput {
            wires_removed: result.wires_removed,
            junctions_removed: result.junctions_removed,
            labels_removed: result.labels_removed,
        })
    }
}

// ============================================================================
// GetNetlist Tool
// ============================================================================

/// Input for get_netlist tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetNetlistInput {
    /// Path to the schematic file
    pub schematic: PathBuf,

    /// Optional component reference to filter by (e.g., "U1"). If not provided, returns all connections.
    #[serde(default)]
    pub filter: Option<String>,
}

/// A single connection entry in the netlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetlistEntry {
    /// Pin reference (e.g., "R1:1")
    pub pin: String,
    /// What this pin is connected to - either other pins or a net label
    pub connections: String,
}

/// Output of get_netlist tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetNetlistOutput {
    /// List of pin connections
    pub entries: Vec<NetlistEntry>,
    /// Number of connections found
    pub count: usize,
}

pub struct GetNetlistTool;

impl Tool for GetNetlistTool {
    const NAME: &'static str = "get_netlist";
    const DESCRIPTION: &'static str = "Get netlist showing pin connections. Returns entries like 'R1:1 - C1:2, U1:10' or 'U1:1 - &GND'. Use filter to show only connections for a specific component.";

    type Input = GetNetlistInput;
    type Output = GetNetlistOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        use crate::connectivity::ConnectivityGraph;
        use std::collections::HashSet;

        let schematic = load_schematic(&input.schematic)?;
        let graph = ConnectivityGraph::from_schematic(&schematic);

        let mut entries: Vec<NetlistEntry> = Vec::new();
        let mut seen_pins: HashSet<String> = HashSet::new();

        // Get all components to iterate over
        let components: Vec<_> = schematic.symbols.iter()
            .filter_map(|sym| {
                sym.properties.iter()
                    .find(|p| p.name == "Reference")
                    .map(|p| (p.value.clone(), sym))
            })
            .collect();

        for (reference, symbol) in &components {
            // Apply filter if specified
            if let Some(ref filter) = input.filter {
                if reference != filter {
                    continue;
                }
            }

            for pin in &symbol.pins {
                let pin_ref = format!("{}:{}", reference, pin.number);

                // Skip if we've already processed this pin from the other side
                if seen_pins.contains(&pin_ref) {
                    continue;
                }

                // Get net name if any
                let net_name = graph.get_net_for_pin(reference, &pin.number);

                // Get other connected pins
                let connected_pins = graph.get_connected_pins(reference, &pin.number);

                // Build connection string
                let connections = if let Some(net) = net_name {
                    // Check if this is a labeled net (not auto-generated NET_n)
                    if !net.starts_with("NET_") {
                        format!("&{}", net)
                    } else if !connected_pins.is_empty() {
                        // Show connected pins for auto-generated nets
                        connected_pins.iter()
                            .map(|(r, p)| format!("{}:{}", r, p))
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        "unconnected".to_string()
                    }
                } else if !connected_pins.is_empty() {
                    connected_pins.iter()
                        .map(|(r, p)| format!("{}:{}", r, p))
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    "unconnected".to_string()
                };

                // Mark connected pins as seen (to avoid duplicates)
                for (r, p) in &connected_pins {
                    seen_pins.insert(format!("{}:{}", r, p));
                }
                seen_pins.insert(pin_ref.clone());

                entries.push(NetlistEntry {
                    pin: pin_ref,
                    connections,
                });
            }
        }

        // Sort entries by pin reference
        entries.sort_by(|a, b| {
            let parse_ref = |s: &str| -> (String, i32, String) {
                let parts: Vec<&str> = s.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let prefix: String = parts[0].chars().take_while(|c| c.is_alphabetic()).collect();
                    let num: i32 = parts[0].chars().skip_while(|c| c.is_alphabetic())
                        .collect::<String>().parse().unwrap_or(0);
                    (prefix, num, parts[1].to_string())
                } else {
                    (s.to_string(), 0, String::new())
                }
            };
            parse_ref(&a.pin).cmp(&parse_ref(&b.pin))
        });

        let count = entries.len();
        Ok(GetNetlistOutput { entries, count })
    }
}

// ============================================================================
// CompileYaml Tool
// ============================================================================

/// Input for compile_yaml tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompileYamlInput {
    /// Path to the YAML schematic definition file
    pub yaml_path: PathBuf,

    /// Output path for the .kicad_sch file (optional, defaults to same name with .kicad_sch extension)
    #[serde(default)]
    pub output_path: Option<PathBuf>,

    /// Only validate the YAML file without compiling
    #[serde(default)]
    pub validate_only: Option<bool>,
}

/// Output of compile_yaml tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileYamlOutput {
    /// Output path of compiled schematic (None if validate_only)
    pub output_path: Option<String>,
    /// Number of components placed
    pub components_placed: usize,
    /// Number of connections made
    pub connections_made: usize,
    /// Validation/compilation warnings
    pub warnings: Vec<String>,
    /// Whether validation passed
    pub is_valid: bool,
    /// Validation errors (if any)
    pub errors: Vec<String>,
}

pub struct CompileYamlTool;

impl Tool for CompileYamlTool {
    const NAME: &'static str = "compile_yaml";
    const DESCRIPTION: &'static str = "Compile a YAML schematic definition to KiCAD format. The YAML file defines components, positions, and connections declaratively.";

    type Input = CompileYamlInput;
    type Output = CompileYamlOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        use crate::yaml::{Compiler, YamlSchematic};

        // Parse the YAML file
        let yaml_sch = YamlSchematic::from_file(&input.yaml_path)
            .map_err(|e| ToolError::InvalidInput(format!("YAML parse error: {}", e)))?;

        // Create the compiler
        let compiler = Compiler::new()
            .map_err(|e| ToolError::ConfigError(format!("KiCAD config error: {}", e)))?;

        if input.validate_only.unwrap_or(false) {
            // Validate only
            let result = compiler.validate(&yaml_sch);
            let is_valid = result.is_valid();
            let errors: Vec<String> = result.errors.iter().map(|e| e.to_string()).collect();
            Ok(CompileYamlOutput {
                output_path: None,
                components_placed: 0,
                connections_made: 0,
                warnings: result.warnings,
                is_valid,
                errors,
            })
        } else {
            // Compile
            // Determine output path
            let output_path = input
                .output_path
                .unwrap_or_else(|| input.yaml_path.with_extension("kicad_sch"));
            // Real KiCad projects derive their `instances` block project name from the
            // .kicad_pro/root schematic file stem; kicaddy has no .kicad_pro of its own, so the
            // output filename is the closest equivalent available here.
            let compiler = match output_path.file_stem().and_then(|s| s.to_str()) {
                Some(stem) => compiler.with_project_name(stem),
                None => compiler,
            };

            match compiler.compile(&yaml_sch) {
                Ok(compile_output) => {
                    // Write the schematic
                    compile_output
                        .schematic
                        .write_to_file(&output_path)
                        .map_err(|e| ToolError::WriteError(e.to_string()))?;

                    Ok(CompileYamlOutput {
                        output_path: Some(output_path.to_string_lossy().to_string()),
                        components_placed: compile_output.components_placed.len(),
                        connections_made: compile_output.connections_made,
                        warnings: compile_output.warnings,
                        is_valid: true,
                        errors: Vec::new(),
                    })
                }
                Err(e) => Ok(CompileYamlOutput {
                    output_path: None,
                    components_placed: 0,
                    connections_made: 0,
                    warnings: Vec::new(),
                    is_valid: false,
                    errors: vec![e.to_string()],
                }),
            }
        }
    }
}

// ============================================================================
// WriteYamlSchematic Tool
// ============================================================================

/// Input for write_yaml_schematic tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteYamlSchematicInput {
    /// Path for the new YAML file
    pub path: PathBuf,

    /// Template to use: "basic", "regulator", or "led" (default: "basic")
    #[serde(default)]
    pub template: Option<String>,

    /// Custom YAML content (overrides template if provided)
    #[serde(default)]
    pub content: Option<String>,
}

/// Output of write_yaml_schematic tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteYamlSchematicOutput {
    /// Path of created YAML file
    pub path: String,
    /// Template used (or "custom" if content was provided)
    pub template: String,
}

pub struct WriteYamlSchematicTool;

impl Tool for WriteYamlSchematicTool {
    const NAME: &'static str = "write_yaml_schematic";
    const DESCRIPTION: &'static str = "Create a new YAML schematic definition file from a template or custom content. Templates: basic (empty), regulator (LDO circuit), led (LED with resistor).";

    type Input = WriteYamlSchematicInput;
    type Output = WriteYamlSchematicOutput;

    fn execute(input: Self::Input) -> Result<Self::Output, ToolError> {
        use crate::yaml::YamlTemplate;

        let (content, template_name) = if let Some(custom_content) = input.content {
            (custom_content, "custom".to_string())
        } else {
            let template_str = input.template.as_deref().unwrap_or("basic");
            let template = YamlTemplate::from_str(template_str)
                .ok_or_else(|| ToolError::InvalidInput(format!(
                    "Unknown template '{}'. Available: basic, regulator, led",
                    template_str
                )))?;
            (template.content().to_string(), template_str.to_string())
        };

        // Write the file
        std::fs::write(&input.path, &content)
            .map_err(|e| ToolError::WriteError(e.to_string()))?;

        Ok(WriteYamlSchematicOutput {
            path: input.path.to_string_lossy().to_string(),
            template: template_name,
        })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate the bounding box for a symbol instance
fn calculate_component_bounds(
    symbol: &SymbolInstance,
    lib_symbol: Option<&Symbol>,
    schematic: &Schematic,
) -> BoundingBox {
    let mut bounds = BoundingBox {
        min_x: f64::MAX,
        min_y: f64::MAX,
        max_x: f64::MIN,
        max_y: f64::MIN,
    };

    let include_point = |bounds: &mut BoundingBox, x: f64, y: f64| {
        bounds.min_x = bounds.min_x.min(x);
        bounds.min_y = bounds.min_y.min(y);
        bounds.max_x = bounds.max_x.max(x);
        bounds.max_y = bounds.max_y.max(y);
    };

    // Get symbol transformation
    let sym_angle_rad = symbol.position.angle.to_radians();
    let cos_a = sym_angle_rad.cos();
    let sin_a = sym_angle_rad.sin();

    // Helper to transform a local point to world coordinates
    let transform_point = |local_x: f64, local_y: f64| -> (f64, f64) {
        let (mirrored_x, mirrored_y) = match symbol.mirror {
            Some(Mirror::X) => (local_x, -local_y),
            Some(Mirror::Y) => (-local_x, local_y),
            None => (local_x, local_y),
        };

        let rotated_x = mirrored_x * cos_a - mirrored_y * sin_a;
        let rotated_y = mirrored_x * sin_a + mirrored_y * cos_a;

        (rotated_x + symbol.position.x, rotated_y + symbol.position.y)
    };

    if let Some(lib_sym) = lib_symbol {
        for unit in &lib_sym.units {
            for graphic in &unit.graphics {
                match graphic {
                    GraphicItem::Rectangle(rect) => {
                        let (x1, y1) = transform_point(rect.start.x, rect.start.y);
                        let (x2, y2) = transform_point(rect.end.x, rect.end.y);
                        include_point(&mut bounds, x1, y1);
                        include_point(&mut bounds, x2, y2);
                    }
                    GraphicItem::Polyline(poly) => {
                        for pt in &poly.points {
                            let (x, y) = transform_point(pt.x, pt.y);
                            include_point(&mut bounds, x, y);
                        }
                    }
                    GraphicItem::Circle(circle) => {
                        let (cx, cy) = transform_point(circle.center.x, circle.center.y);
                        include_point(&mut bounds, cx - circle.radius, cy - circle.radius);
                        include_point(&mut bounds, cx + circle.radius, cy + circle.radius);
                    }
                    GraphicItem::Arc(arc) => {
                        let (x1, y1) = transform_point(arc.start.x, arc.start.y);
                        let (x2, y2) = transform_point(arc.mid.x, arc.mid.y);
                        let (x3, y3) = transform_point(arc.end.x, arc.end.y);
                        include_point(&mut bounds, x1, y1);
                        include_point(&mut bounds, x2, y2);
                        include_point(&mut bounds, x3, y3);
                    }
                    GraphicItem::Text(_) => {}
                }
            }

            for pin in &unit.pins {
                let pin_pos = pin.position;
                let pin_length = pin.length;
                let pin_angle_rad = pin_pos.angle.to_radians();

                let (base_x, base_y) = transform_point(pin_pos.x, pin_pos.y);
                include_point(&mut bounds, base_x, base_y);

                let tip_local_x = pin_pos.x - pin_length * pin_angle_rad.cos();
                let tip_local_y = pin_pos.y + pin_length * pin_angle_rad.sin();
                let (tip_x, tip_y) = transform_point(tip_local_x, tip_local_y);
                include_point(&mut bounds, tip_x, tip_y);
            }
        }
    }

    // Fallback: use pin positions from schematic
    if bounds.min_x > bounds.max_x {
        for (pos, _) in schematic.get_all_pin_positions(symbol) {
            include_point(&mut bounds, pos.x, pos.y);
        }
    }

    // Final fallback: use symbol center
    if bounds.min_x > bounds.max_x {
        include_point(&mut bounds, symbol.position.x, symbol.position.y);
    }

    bounds
}
