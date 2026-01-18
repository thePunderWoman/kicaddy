//! Tool abstraction for shared CLI/MCP functionality
//!
//! Each tool is stateless and takes a schematic path as input for editing operations.
//! Tools provide JSON schema generation via schemars for MCP integration.

pub mod outline;
pub mod update_component;
pub mod wrappers;

pub use outline::{OutlineTool, OutlineInput, OutlineOutput};
pub use update_component::{UpdateComponentTool, UpdateComponentInput, UpdateComponentOutput};

use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during tool execution
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Failed to read schematic: {0}")]
    ReadError(String),

    #[error("Failed to write schematic: {0}")]
    WriteError(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("Component not found: {0}")]
    ComponentNotFound(String),

    #[error("Pin not found: {reference}:{pin}")]
    PinNotFound { reference: String, pin: String },

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Library lookup failed: {0}")]
    LookupError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Command error: {0}")]
    CommandError(String),

    #[error("{0}")]
    Other(String),
}

impl From<crate::commands::CommandError> for ToolError {
    fn from(e: crate::commands::CommandError) -> Self {
        ToolError::CommandError(e.to_string())
    }
}

impl From<crate::SchematicError> for ToolError {
    fn from(e: crate::SchematicError) -> Self {
        ToolError::ReadError(e.to_string())
    }
}

impl From<crate::config::ConfigError> for ToolError {
    fn from(e: crate::config::ConfigError) -> Self {
        ToolError::ConfigError(e.to_string())
    }
}

impl From<crate::LookupError> for ToolError {
    fn from(e: crate::LookupError) -> Self {
        ToolError::LookupError(e.to_string())
    }
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        ToolError::Other(e.to_string())
    }
}

/// The Tool trait for stateless schematic operations
///
/// Each tool:
/// - Has a name and description for MCP tool registration
/// - Takes a strongly-typed Input (with JsonSchema support)
/// - Returns a strongly-typed Output (JSON serializable)
/// - Is stateless - each call reads/writes the schematic file
pub trait Tool {
    /// Unique tool name (used for MCP registration)
    const NAME: &'static str;

    /// Human-readable description of what the tool does
    const DESCRIPTION: &'static str;

    /// Input type with JSON schema support
    type Input: Serialize + DeserializeOwned + JsonSchema;

    /// Output type (JSON serializable)
    type Output: Serialize;

    /// Execute the tool with the given input
    fn execute(input: Self::Input) -> Result<Self::Output, ToolError>;

    /// Generate JSON schema for the input type
    fn input_schema() -> serde_json::Value {
        let schema = schemars::schema_for!(Self::Input);
        serde_json::to_value(schema).unwrap_or_default()
    }
}

/// Helper to load a schematic from a path
pub fn load_schematic(path: &PathBuf) -> Result<crate::Schematic, ToolError> {
    crate::parse_schematic(path).map_err(|e| ToolError::ReadError(e.to_string()))
}

/// Helper to save a schematic to a path
pub fn save_schematic(schematic: &crate::Schematic, path: &PathBuf) -> Result<(), ToolError> {
    schematic
        .write_to_file(path)
        .map_err(|e| ToolError::WriteError(e.to_string()))
}

/// Registry of all available tools for MCP server
pub struct ToolRegistry;

impl ToolRegistry {
    /// Get metadata for all registered tools
    pub fn tools() -> Vec<ToolMetadata> {
        vec![
            ToolMetadata {
                name: OutlineTool::NAME.to_string(),
                description: OutlineTool::DESCRIPTION.to_string(),
                input_schema: OutlineTool::input_schema(),
            },
            ToolMetadata {
                name: UpdateComponentTool::NAME.to_string(),
                description: UpdateComponentTool::DESCRIPTION.to_string(),
                input_schema: UpdateComponentTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::PlaceComponentTool::NAME.to_string(),
                description: wrappers::PlaceComponentTool::DESCRIPTION.to_string(),
                input_schema: wrappers::PlaceComponentTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::DeleteComponentTool::NAME.to_string(),
                description: wrappers::DeleteComponentTool::DESCRIPTION.to_string(),
                input_schema: wrappers::DeleteComponentTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::AddWireTool::NAME.to_string(),
                description: wrappers::AddWireTool::DESCRIPTION.to_string(),
                input_schema: wrappers::AddWireTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::DeleteWireTool::NAME.to_string(),
                description: wrappers::DeleteWireTool::DESCRIPTION.to_string(),
                input_schema: wrappers::DeleteWireTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::AddLabelTool::NAME.to_string(),
                description: wrappers::AddLabelTool::DESCRIPTION.to_string(),
                input_schema: wrappers::AddLabelTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::DeleteLabelTool::NAME.to_string(),
                description: wrappers::DeleteLabelTool::DESCRIPTION.to_string(),
                input_schema: wrappers::DeleteLabelTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::SearchSymbolTool::NAME.to_string(),
                description: wrappers::SearchSymbolTool::DESCRIPTION.to_string(),
                input_schema: wrappers::SearchSymbolTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::GetConfigTool::NAME.to_string(),
                description: wrappers::GetConfigTool::DESCRIPTION.to_string(),
                input_schema: wrappers::GetConfigTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::NewSchematicTool::NAME.to_string(),
                description: wrappers::NewSchematicTool::DESCRIPTION.to_string(),
                input_schema: wrappers::NewSchematicTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::ConnectTool::NAME.to_string(),
                description: wrappers::ConnectTool::DESCRIPTION.to_string(),
                input_schema: wrappers::ConnectTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::DisconnectTool::NAME.to_string(),
                description: wrappers::DisconnectTool::DESCRIPTION.to_string(),
                input_schema: wrappers::DisconnectTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::GetNetlistTool::NAME.to_string(),
                description: wrappers::GetNetlistTool::DESCRIPTION.to_string(),
                input_schema: wrappers::GetNetlistTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::CompileYamlTool::NAME.to_string(),
                description: wrappers::CompileYamlTool::DESCRIPTION.to_string(),
                input_schema: wrappers::CompileYamlTool::input_schema(),
            },
            ToolMetadata {
                name: wrappers::WriteYamlSchematicTool::NAME.to_string(),
                description: wrappers::WriteYamlSchematicTool::DESCRIPTION.to_string(),
                input_schema: wrappers::WriteYamlSchematicTool::input_schema(),
            },
        ]
    }

    /// Execute a tool by name with JSON input, returning JSON output
    pub fn execute(name: &str, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        match name {
            OutlineTool::NAME => {
                let input: OutlineInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = OutlineTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            UpdateComponentTool::NAME => {
                let input: UpdateComponentInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = UpdateComponentTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::PlaceComponentTool::NAME => {
                let input: wrappers::PlaceComponentInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::PlaceComponentTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::DeleteComponentTool::NAME => {
                let input: wrappers::DeleteComponentInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::DeleteComponentTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::AddWireTool::NAME => {
                let input: wrappers::AddWireInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::AddWireTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::DeleteWireTool::NAME => {
                let input: wrappers::DeleteWireInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::DeleteWireTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::AddLabelTool::NAME => {
                let input: wrappers::AddLabelInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::AddLabelTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::DeleteLabelTool::NAME => {
                let input: wrappers::DeleteLabelInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::DeleteLabelTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::SearchSymbolTool::NAME => {
                let input: wrappers::SearchSymbolInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::SearchSymbolTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::GetConfigTool::NAME => {
                let input: wrappers::GetConfigInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::GetConfigTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::NewSchematicTool::NAME => {
                let input: wrappers::NewSchematicInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::NewSchematicTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::ConnectTool::NAME => {
                let input: wrappers::ConnectInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::ConnectTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::DisconnectTool::NAME => {
                let input: wrappers::DisconnectInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::DisconnectTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::GetNetlistTool::NAME => {
                let input: wrappers::GetNetlistInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::GetNetlistTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::CompileYamlTool::NAME => {
                let input: wrappers::CompileYamlInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::CompileYamlTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            wrappers::WriteYamlSchematicTool::NAME => {
                let input: wrappers::WriteYamlSchematicInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
                let output = wrappers::WriteYamlSchematicTool::execute(input)?;
                serde_json::to_value(output).map_err(|e| ToolError::Other(e.to_string()))
            }
            _ => Err(ToolError::Other(format!("Unknown tool: {}", name))),
        }
    }
}

/// Metadata about a tool for MCP registration
#[derive(Debug, Clone, Serialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
