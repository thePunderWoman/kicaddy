//! Connectivity graph for schematic analysis
//!
//! Builds and maintains connectivity information between pins, wires, and labels.

use std::collections::HashMap;

use crate::common::Point;
use crate::schematic::Schematic;

/// Connectivity graph built from a schematic
#[derive(Debug)]
pub struct ConnectivityGraph {
    /// All connection points in the schematic
    connection_points: Vec<ConnectionPoint>,
    /// Union-find parent array
    parent: Vec<usize>,
    /// Union-find rank array
    rank: Vec<usize>,
    /// Net groups: root index -> list of point indices
    net_groups: HashMap<usize, Vec<usize>>,
    /// Pin to net name mapping
    pin_to_net: HashMap<(String, String), String>,
}

/// A connection point in the schematic
#[derive(Debug, Clone)]
pub struct ConnectionPoint {
    /// Position in schematic coordinates
    pub position: Point,
    /// Kind of connection point
    pub kind: ConnectionKind,
}

/// Kind of connection point
#[derive(Debug, Clone)]
pub enum ConnectionKind {
    /// Pin on a component
    Pin {
        reference: String,
        pin_number: String,
        pin_name: String,
    },
    /// Wire endpoint
    WireEndpoint,
    /// Junction
    Junction,
    /// Net label (local or global)
    Label { name: String, is_global: bool },
}

impl ConnectivityGraph {
    /// Tolerance for matching connection points (in mm)
    /// Very small - just for floating-point comparison, not for "nearby" matching
    const TOLERANCE: f64 = 0.01;

    /// Build a connectivity graph from a schematic
    pub fn from_schematic(schematic: &Schematic) -> Self {
        let mut graph = Self {
            connection_points: Vec::new(),
            parent: Vec::new(),
            rank: Vec::new(),
            net_groups: HashMap::new(),
            pin_to_net: HashMap::new(),
        };

        graph.collect_connection_points(schematic);
        graph.initialize_union_find();
        graph.connect_at_junctions(schematic);
        graph.connect_wire_segments(schematic);
        graph.connect_labels_to_wires();
        graph.build_net_groups();
        graph.assign_net_names();

        graph
    }

    /// Collect all connection points from the schematic
    fn collect_connection_points(&mut self, schematic: &Schematic) {
        // Add pin positions
        for symbol in &schematic.symbols {
            let reference = symbol
                .properties
                .iter()
                .find(|p| p.name == "Reference")
                .map(|p| p.value.clone())
                .unwrap_or_else(|| "?".to_string());

            // Get lib_symbol for pin names
            let lib_symbol = schematic.lib_symbols.iter().find(|s| s.name == symbol.lib_id);

            for pin in &symbol.pins {
                if let Some((pos, _angle)) = schematic.get_pin_position(symbol, &pin.number) {
                    let pin_name = lib_symbol
                        .and_then(|s| {
                            s.units
                                .iter()
                                .flat_map(|u| u.pins.iter())
                                .find(|p| p.number.number == pin.number)
                                .map(|p| p.name.name.clone())
                        })
                        .unwrap_or_else(|| pin.number.clone());

                    self.connection_points.push(ConnectionPoint {
                        position: pos,
                        kind: ConnectionKind::Pin {
                            reference: reference.clone(),
                            pin_number: pin.number.clone(),
                            pin_name,
                        },
                    });
                }
            }
        }

        // Add wire start/end points only (not intermediate corners)
        // This prevents false connections when orthogonal wires cross
        for wire in &schematic.wires {
            if let Some(first) = wire.points.first() {
                self.connection_points.push(ConnectionPoint {
                    position: *first,
                    kind: ConnectionKind::WireEndpoint,
                });
            }
            if wire.points.len() > 1 {
                if let Some(last) = wire.points.last() {
                    self.connection_points.push(ConnectionPoint {
                        position: *last,
                        kind: ConnectionKind::WireEndpoint,
                    });
                }
            }
        }

        // Add junctions
        for junction in &schematic.junctions {
            self.connection_points.push(ConnectionPoint {
                position: junction.position,
                kind: ConnectionKind::Junction,
            });
        }

        // Add local labels
        for label in &schematic.labels {
            self.connection_points.push(ConnectionPoint {
                position: Point::new(label.position.x, label.position.y),
                kind: ConnectionKind::Label {
                    name: label.text.clone(),
                    is_global: false,
                },
            });
        }

        // Add global labels
        for label in &schematic.global_labels {
            self.connection_points.push(ConnectionPoint {
                position: Point::new(label.position.x, label.position.y),
                kind: ConnectionKind::Label {
                    name: label.text.clone(),
                    is_global: true,
                },
            });
        }
    }

    /// Initialize union-find data structure
    fn initialize_union_find(&mut self) {
        let n = self.connection_points.len();
        self.parent = (0..n).collect();
        self.rank = vec![0; n];
    }

    /// Find root with path compression
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    /// Union by rank
    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx != ry {
            match self.rank[rx].cmp(&self.rank[ry]) {
                std::cmp::Ordering::Less => self.parent[rx] = ry,
                std::cmp::Ordering::Greater => self.parent[ry] = rx,
                std::cmp::Ordering::Equal => {
                    self.parent[ry] = rx;
                    self.rank[rx] += 1;
                }
            }
        }
    }

    /// Connect points at junction positions
    /// In KiCAD, junctions explicitly mark where wires connect
    fn connect_at_junctions(&mut self, schematic: &Schematic) {
        for junction in &schematic.junctions {
            let jp = junction.position;

            // Find all connection points at this junction position
            let indices_at_junction: Vec<usize> = self.connection_points
                .iter()
                .enumerate()
                .filter(|(_, cp)| self.points_match(cp.position, jp))
                .map(|(i, _)| i)
                .collect();

            // Connect all points at this junction
            for i in 1..indices_at_junction.len() {
                self.union(indices_at_junction[0], indices_at_junction[i]);
            }
        }
    }

    /// Connect labels to wire endpoints at the same position
    fn connect_labels_to_wires(&mut self) {
        let n = self.connection_points.len();

        // Collect all label-to-point connections first
        let mut connections: Vec<(usize, usize)> = Vec::new();

        for i in 0..n {
            // Only process labels
            if !matches!(self.connection_points[i].kind, ConnectionKind::Label { .. }) {
                continue;
            }
            let label_pos = self.connection_points[i].position;

            // Find wire endpoints or pins at the same position
            for j in 0..n {
                if i == j {
                    continue;
                }
                match &self.connection_points[j].kind {
                    ConnectionKind::WireEndpoint | ConnectionKind::Pin { .. } => {
                        let other_pos = self.connection_points[j].position;
                        let dx = label_pos.x - other_pos.x;
                        let dy = label_pos.y - other_pos.y;
                        if dx * dx + dy * dy < Self::TOLERANCE * Self::TOLERANCE {
                            connections.push((i, j));
                        }
                    }
                    _ => {}
                }
            }
        }

        // Apply all connections
        for (i, j) in connections {
            self.union(i, j);
        }
    }

    /// Check if two points are at the same position (within floating-point tolerance)
    fn points_match(&self, p1: Point, p2: Point) -> bool {
        let dx = p1.x - p2.x;
        let dy = p1.y - p2.y;
        dx * dx + dy * dy < Self::TOLERANCE * Self::TOLERANCE
    }

    /// Connect wire endpoints to pins at the same position, and wire segments together
    fn connect_wire_segments(&mut self, schematic: &Schematic) {
        for wire in &schematic.wires {
            // Get wire start and end positions
            let start = wire.points.first();
            let end = wire.points.last();

            // Connect start to any pin at that position
            if let Some(start_pos) = start {
                self.connect_point_to_pins(*start_pos);
            }

            // Connect end to any pin at that position
            if let Some(end_pos) = end {
                if wire.points.len() > 1 {
                    self.connect_point_to_pins(*end_pos);
                }
            }

            // Connect start and end of the same wire (they're on the same net)
            if let (Some(sp), Some(ep)) = (start, end) {
                if wire.points.len() > 1 {
                    let idx1 = self.find_point_index(*sp);
                    let idx2 = self.find_point_index(*ep);
                    if let (Some(i1), Some(i2)) = (idx1, idx2) {
                        self.union(i1, i2);
                    }
                }
            }
        }
    }

    /// Connect a point to any pins at the same position
    fn connect_point_to_pins(&mut self, pos: Point) {
        let point_idx = self.find_point_index(pos);
        if point_idx.is_none() {
            return;
        }
        let point_idx = point_idx.unwrap();

        // Find pins at this position (collect indices first to avoid borrow issues)
        let pin_indices: Vec<usize> = self.connection_points
            .iter()
            .enumerate()
            .filter(|(_, cp)| {
                matches!(cp.kind, ConnectionKind::Pin { .. }) && {
                    let dx = cp.position.x - pos.x;
                    let dy = cp.position.y - pos.y;
                    dx * dx + dy * dy < Self::TOLERANCE * Self::TOLERANCE
                }
            })
            .map(|(i, _)| i)
            .collect();

        // Union all pins at this position with the wire endpoint
        for pin_idx in pin_indices {
            self.union(point_idx, pin_idx);
        }
    }

    /// Find index of connection point at position
    fn find_point_index(&self, pos: Point) -> Option<usize> {
        self.connection_points.iter().position(|cp| {
            let dx = cp.position.x - pos.x;
            let dy = cp.position.y - pos.y;
            dx * dx + dy * dy < Self::TOLERANCE * Self::TOLERANCE
        })
    }

    /// Build net groups from union-find structure
    fn build_net_groups(&mut self) {
        self.net_groups.clear();
        for i in 0..self.connection_points.len() {
            let root = self.find(i);
            self.net_groups.entry(root).or_default().push(i);
        }
    }

    /// Assign net names to groups
    fn assign_net_names(&mut self) {
        self.pin_to_net.clear();
        let mut auto_net_counter = 1;

        for (_root, indices) in &self.net_groups {
            let mut pins: Vec<(String, String)> = Vec::new();
            let mut label_name: Option<(String, bool)> = None;

            for &idx in indices {
                match &self.connection_points[idx].kind {
                    ConnectionKind::Pin {
                        reference,
                        pin_number,
                        ..
                    } => {
                        pins.push((reference.clone(), pin_number.clone()));
                    }
                    ConnectionKind::Label { name, is_global } => {
                        // Prefer global labels over local labels
                        match &label_name {
                            None => label_name = Some((name.clone(), *is_global)),
                            Some((_, false)) if *is_global => {
                                label_name = Some((name.clone(), true))
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            // Skip groups with no pins
            if pins.is_empty() {
                continue;
            }

            // Determine net name
            let net_name = match label_name {
                Some((name, _)) => name,
                None => {
                    let name = format!("NET_{}", auto_net_counter);
                    auto_net_counter += 1;
                    name
                }
            };

            // Map pins to net
            for (ref_, pin) in &pins {
                self.pin_to_net
                    .insert((ref_.clone(), pin.clone()), net_name.clone());
            }
        }
    }

    /// Get the net name for a pin
    pub fn get_net_for_pin(&self, reference: &str, pin: &str) -> Option<&String> {
        self.pin_to_net.get(&(reference.to_string(), pin.to_string()))
    }

    /// Get all pins connected to a pin (excluding itself)
    pub fn get_connected_pins(&self, reference: &str, pin: &str) -> Vec<(String, String)> {
        // Find the pin's index
        let pin_idx = self.connection_points.iter().position(|cp| {
            matches!(&cp.kind, ConnectionKind::Pin { reference: r, pin_number: p, .. }
                if r == reference && p == pin)
        });

        let pin_idx = match pin_idx {
            Some(idx) => idx,
            None => return vec![],
        };

        // Find all pins in the same net group
        // Need to find the root - but we can't call find() because we only have &self
        // So we need to trace the parent chain manually
        let mut root = pin_idx;
        while self.parent[root] != root {
            root = self.parent[root];
        }

        let indices = match self.net_groups.get(&root) {
            Some(indices) => indices,
            None => return vec![],
        };

        // Collect all other pins in this group
        indices
            .iter()
            .filter_map(|&idx| {
                if idx == pin_idx {
                    return None;
                }
                match &self.connection_points[idx].kind {
                    ConnectionKind::Pin {
                        reference: r,
                        pin_number: p,
                        ..
                    } => Some((r.clone(), p.clone())),
                    _ => None,
                }
            })
            .collect()
    }

    /// Check if two points are connected
    pub fn are_connected(&self, pos1: Point, pos2: Point) -> bool {
        let idx1 = self.find_point_index(pos1);
        let idx2 = self.find_point_index(pos2);

        match (idx1, idx2) {
            (Some(i1), Some(i2)) => {
                // Trace to roots without mutation
                let mut root1 = i1;
                while self.parent[root1] != root1 {
                    root1 = self.parent[root1];
                }
                let mut root2 = i2;
                while self.parent[root2] != root2 {
                    root2 = self.parent[root2];
                }
                root1 == root2
            }
            _ => false,
        }
    }

    /// Get all connection points
    pub fn connection_points(&self) -> &[ConnectionPoint] {
        &self.connection_points
    }

    /// Get the pin to net mapping
    pub fn pin_to_net(&self) -> &HashMap<(String, String), String> {
        &self.pin_to_net
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_schematic() {
        let schematic = Schematic::new();
        let graph = ConnectivityGraph::from_schematic(&schematic);
        assert!(graph.connection_points.is_empty());
        assert!(graph.pin_to_net.is_empty());
    }
}
