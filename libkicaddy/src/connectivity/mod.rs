//! Connectivity module for logical connection modeling
//!
//! This module provides:
//! - Endpoint parsing for pin references (`R1:1`) and net labels (`&GND`)
//! - Connectivity graph building from schematic elements
//! - Connection and disconnection operations

pub mod endpoint;
pub mod graph;

pub use endpoint::{ConnectionEndpoint, ConnectionError};
pub use graph::ConnectivityGraph;
