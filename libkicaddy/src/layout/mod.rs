//! Force-directed layout module for automatic component positioning
//!
//! This module provides automatic placement of components in schematics using
//! a force-directed graph layout algorithm (Fruchterman-Reingold variant).
//!
//! # Overview
//!
//! The layout algorithm treats components as graph nodes and wire connections as edges:
//! - **Repulsive forces** push all nodes apart to prevent overlap
//! - **Attractive forces** pull connected nodes together
//! - **Group affinity** clusters components in the same YAML group
//! - **Pin direction awareness** biases positions so wires exit pins naturally
//!
//! # Example
//!
//! ```ignore
//! use libkicaddy::layout::{LayoutGraph, LayoutNode, LayoutEdge, ForceDirectedLayout};
//!
//! let mut graph = LayoutGraph::new();
//!
//! // Add nodes (components)
//! graph.add_node(LayoutNode::new("R1".to_string(), (10.0, 5.0)));
//! graph.add_node(LayoutNode::new("R2".to_string(), (10.0, 5.0)));
//!
//! // Add edges (connections)
//! graph.add_edge(LayoutEdge::new(
//!     "R1".to_string(), "2".to_string(),
//!     "R2".to_string(), "1".to_string(),
//! ));
//!
//! // Run layout
//! let layout = ForceDirectedLayout::new();
//! layout.layout(&mut graph);
//!
//! // Get computed positions
//! let r1_pos = graph.get_node("R1").unwrap().position;
//! let r2_pos = graph.get_node("R2").unwrap().position;
//! ```

pub mod config;
pub mod force;
pub mod graph;

pub use config::{paper_dimensions, snap_to_grid, LayoutConfig, GRID_SIZE_MM};
pub use force::ForceDirectedLayout;
pub use graph::{LayoutEdge, LayoutGraph, LayoutNode, PinInfo};
