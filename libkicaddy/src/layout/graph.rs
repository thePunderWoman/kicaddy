//! Graph data structures for layout computation

use std::collections::HashMap;

use crate::common::Point;

/// Information about a pin on a component
#[derive(Debug, Clone)]
pub struct PinInfo {
    /// Offset from node center to pin position
    pub offset: Point,
    /// Direction the pin points (0=right, 90=up, 180=left, 270=down)
    /// This is the direction toward the symbol body, so wires should exit opposite
    pub direction: f64,
    /// Pin number for identification
    pub number: String,
    /// Pin name (may be empty or "~")
    pub name: String,
}

impl PinInfo {
    /// Get the preferred direction for a wire exiting this pin
    /// Returns a unit vector (dx, dy) in the direction the wire should go
    pub fn wire_direction(&self) -> (f64, f64) {
        // Pin direction points toward symbol body, wire exits opposite
        let rad = (self.direction + 180.0).to_radians();
        (rad.cos(), -rad.sin()) // Negate Y because screen coords are Y-down
    }
}

/// A node in the layout graph (represents a component)
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Component reference (e.g., "R1", "U1")
    pub reference: String,
    /// Current position (center of component)
    pub position: Point,
    /// Bounding box size (width, height) in mm
    pub size: (f64, f64),
    /// Velocity for physics simulation
    pub velocity: Point,
    /// Whether this node's position is fixed (user specified)
    pub fixed: bool,
    /// Group name for affinity clustering (None = top-level)
    pub group: Option<String>,
    /// Pin information keyed by pin number/name
    pub pins: HashMap<String, PinInfo>,
}

impl LayoutNode {
    /// Create a new layout node
    pub fn new(reference: String, size: (f64, f64)) -> Self {
        Self {
            reference,
            position: Point::default(),
            size,
            velocity: Point::default(),
            fixed: false,
            group: None,
            pins: HashMap::new(),
        }
    }

    /// Set the position
    pub fn with_position(mut self, x: f64, y: f64) -> Self {
        self.position = Point::new(x, y);
        self
    }

    /// Mark as fixed (user-specified position)
    pub fn with_fixed(mut self, fixed: bool) -> Self {
        self.fixed = fixed;
        self
    }

    /// Set the group
    pub fn with_group(mut self, group: Option<String>) -> Self {
        self.group = group;
        self
    }

    /// Add a pin
    pub fn add_pin(&mut self, key: String, pin: PinInfo) {
        self.pins.insert(key, pin);
    }

    /// Get the bounding box (min_x, min_y, max_x, max_y)
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let half_w = self.size.0 / 2.0;
        let half_h = self.size.1 / 2.0;
        (
            self.position.x - half_w,
            self.position.y - half_h,
            self.position.x + half_w,
            self.position.y + half_h,
        )
    }

    /// Check if this node's bounding box overlaps with another
    pub fn overlaps(&self, other: &LayoutNode, padding: f64) -> bool {
        let (ax1, ay1, ax2, ay2) = self.bounds();
        let (bx1, by1, bx2, by2) = other.bounds();

        !(ax2 + padding < bx1
            || bx2 + padding < ax1
            || ay2 + padding < by1
            || by2 + padding < ay1)
    }

    /// Calculate the minimum separation vector to resolve overlap with another node.
    /// Returns (overlap_depth, separation_direction) or None if not overlapping.
    /// separation_direction is a unit vector pointing from other toward self.
    pub fn separation_from(&self, other: &LayoutNode, padding: f64) -> Option<(f64, Point)> {
        let (ax1, ay1, ax2, ay2) = self.bounds();
        let (bx1, by1, bx2, by2) = other.bounds();

        // Add padding to self's bounds (expand outward)
        let ax1 = ax1 - padding;
        let ay1 = ay1 - padding;
        let ax2 = ax2 + padding;
        let ay2 = ay2 + padding;

        // Calculate overlap on each axis
        let overlap_x = (ax2.min(bx2) - ax1.max(bx1)).max(0.0);
        let overlap_y = (ay2.min(by2) - ay1.max(by1)).max(0.0);

        if overlap_x <= 0.0 || overlap_y <= 0.0 {
            return None; // No overlap
        }

        // Separate along axis with smaller overlap (minimum translation vector)
        if overlap_x < overlap_y {
            // Separate horizontally
            let dir_x = if self.position.x > other.position.x {
                1.0
            } else {
                -1.0
            };
            Some((overlap_x, Point::new(dir_x, 0.0)))
        } else {
            // Separate vertically
            let dir_y = if self.position.y > other.position.y {
                1.0
            } else {
                -1.0
            };
            Some((overlap_y, Point::new(0.0, dir_y)))
        }
    }

    /// Calculate the gap between bounding boxes (0 if overlapping, positive if separated)
    pub fn gap_from(&self, other: &LayoutNode) -> f64 {
        let (ax1, ay1, ax2, ay2) = self.bounds();
        let (bx1, by1, bx2, by2) = other.bounds();

        // Gap on each axis (negative means overlap)
        let gap_x = if ax2 < bx1 {
            bx1 - ax2
        } else if bx2 < ax1 {
            ax1 - bx2
        } else {
            0.0
        };
        let gap_y = if ay2 < by1 {
            by1 - ay2
        } else if by2 < ay1 {
            ay1 - by2
        } else {
            0.0
        };

        // If separated on either axis, return Euclidean distance between closest corners
        if gap_x > 0.0 || gap_y > 0.0 {
            (gap_x * gap_x + gap_y * gap_y).sqrt()
        } else {
            0.0 // Overlapping
        }
    }

    /// Get world position of a pin by key
    pub fn pin_world_position(&self, pin_key: &str) -> Option<Point> {
        self.pins.get(pin_key).map(|pin| {
            Point::new(
                self.position.x + pin.offset.x,
                self.position.y + pin.offset.y,
            )
        })
    }
}

/// An edge in the layout graph (represents a wire connection)
#[derive(Debug, Clone)]
pub struct LayoutEdge {
    /// Source node reference
    pub from_node: String,
    /// Source pin key
    pub from_pin: String,
    /// Target node reference
    pub to_node: String,
    /// Target pin key
    pub to_pin: String,
}

impl LayoutEdge {
    /// Create a new layout edge
    pub fn new(from_node: String, from_pin: String, to_node: String, to_pin: String) -> Self {
        Self {
            from_node,
            from_pin,
            to_node,
            to_pin,
        }
    }
}

/// The layout graph containing all nodes and edges
#[derive(Debug, Default)]
pub struct LayoutGraph {
    /// All nodes keyed by reference
    pub nodes: HashMap<String, LayoutNode>,
    /// All edges
    pub edges: Vec<LayoutEdge>,
}

impl LayoutGraph {
    /// Create a new empty layout graph
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: LayoutNode) {
        self.nodes.insert(node.reference.clone(), node);
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, edge: LayoutEdge) {
        self.edges.push(edge);
    }

    /// Get a mutable reference to a node
    pub fn get_node_mut(&mut self, reference: &str) -> Option<&mut LayoutNode> {
        self.nodes.get_mut(reference)
    }

    /// Get a reference to a node
    pub fn get_node(&self, reference: &str) -> Option<&LayoutNode> {
        self.nodes.get(reference)
    }

    /// Get all nodes in a specific group
    pub fn nodes_in_group(&self, group: Option<&str>) -> Vec<&LayoutNode> {
        self.nodes
            .values()
            .filter(|n| n.group.as_deref() == group)
            .collect()
    }

    /// Get all unique group names
    pub fn groups(&self) -> Vec<Option<String>> {
        let mut groups: Vec<_> = self
            .nodes
            .values()
            .map(|n| n.group.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        groups.sort();
        groups
    }

    /// Calculate the centroid of a group of nodes
    pub fn group_centroid(&self, group: Option<&str>) -> Option<Point> {
        let nodes: Vec<_> = self.nodes_in_group(group);
        if nodes.is_empty() {
            return None;
        }

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        for node in &nodes {
            sum_x += node.position.x;
            sum_y += node.position.y;
        }

        Some(Point::new(
            sum_x / nodes.len() as f64,
            sum_y / nodes.len() as f64,
        ))
    }

    /// Get edges connected to a specific node
    pub fn edges_for_node(&self, reference: &str) -> Vec<&LayoutEdge> {
        self.edges
            .iter()
            .filter(|e| e.from_node == reference || e.to_node == reference)
            .collect()
    }

    /// Check if two nodes are directly connected
    pub fn are_connected(&self, ref1: &str, ref2: &str) -> bool {
        self.edges.iter().any(|e| {
            (e.from_node == ref1 && e.to_node == ref2)
                || (e.from_node == ref2 && e.to_node == ref1)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_bounds() {
        let node = LayoutNode::new("R1".to_string(), (10.0, 5.0)).with_position(50.0, 30.0);

        let (x1, y1, x2, y2) = node.bounds();
        assert_eq!(x1, 45.0);
        assert_eq!(y1, 27.5);
        assert_eq!(x2, 55.0);
        assert_eq!(y2, 32.5);
    }

    #[test]
    fn test_node_overlap() {
        let node1 = LayoutNode::new("R1".to_string(), (10.0, 10.0)).with_position(0.0, 0.0);

        let node2 = LayoutNode::new("R2".to_string(), (10.0, 10.0)).with_position(8.0, 0.0);

        let node3 = LayoutNode::new("R3".to_string(), (10.0, 10.0)).with_position(20.0, 0.0);

        assert!(node1.overlaps(&node2, 0.0)); // overlapping (gap = 3 units)
        assert!(!node1.overlaps(&node3, 0.0)); // not overlapping (gap = 10 units)
        assert!(!node1.overlaps(&node3, 6.0)); // still not overlapping (padding 6 < gap 10)
        assert!(node1.overlaps(&node3, 11.0)); // overlapping with padding 11 >= gap 10
    }

    #[test]
    fn test_pin_wire_direction() {
        // Pin pointing right (toward symbol) -> wire exits left
        let pin_right = PinInfo {
            offset: Point::default(),
            direction: 0.0,
            number: "1".to_string(),
            name: "".to_string(),
        };
        let (dx, dy) = pin_right.wire_direction();
        assert!((dx - (-1.0)).abs() < 0.001); // wire goes left
        assert!(dy.abs() < 0.001);

        // Pin pointing left (toward symbol) -> wire exits right
        let pin_left = PinInfo {
            offset: Point::default(),
            direction: 180.0,
            number: "2".to_string(),
            name: "".to_string(),
        };
        let (dx, dy) = pin_left.wire_direction();
        assert!((dx - 1.0).abs() < 0.001); // wire goes right
        assert!(dy.abs() < 0.001);
    }

    #[test]
    fn test_graph_groups() {
        let mut graph = LayoutGraph::new();

        graph.add_node(
            LayoutNode::new("R1".to_string(), (5.0, 2.0)).with_group(Some("Power".to_string())),
        );
        graph.add_node(
            LayoutNode::new("R2".to_string(), (5.0, 2.0)).with_group(Some("Power".to_string())),
        );
        graph.add_node(LayoutNode::new("U1".to_string(), (10.0, 8.0)).with_group(None));

        let power_nodes = graph.nodes_in_group(Some("Power"));
        assert_eq!(power_nodes.len(), 2);

        let top_level_nodes = graph.nodes_in_group(None);
        assert_eq!(top_level_nodes.len(), 1);
    }

    #[test]
    fn test_separation_from_overlapping() {
        // Two 10x10 nodes at (0,0) and (8,0) - they overlap horizontally
        let node1 = LayoutNode::new("R1".to_string(), (10.0, 10.0)).with_position(0.0, 0.0);
        let node2 = LayoutNode::new("R2".to_string(), (10.0, 10.0)).with_position(8.0, 0.0);

        // node1 bounds: (-5,-5) to (5,5)
        // node2 bounds: (3,-5) to (13,5)
        // overlap_x = min(5,13) - max(-5,3) = 5 - 3 = 2
        // overlap_y = min(5,5) - max(-5,-5) = 5 - (-5) = 10
        // Since overlap_x < overlap_y, separate horizontally
        let result = node1.separation_from(&node2, 0.0);
        assert!(result.is_some());
        let (depth, dir) = result.unwrap();
        assert!((depth - 2.0).abs() < 0.001, "Expected depth 2.0, got {}", depth);
        assert!((dir.x - (-1.0)).abs() < 0.001, "Expected dir.x -1.0, got {}", dir.x);
        assert!(dir.y.abs() < 0.001, "Expected dir.y 0.0, got {}", dir.y);
    }

    #[test]
    fn test_separation_from_not_overlapping() {
        // Two 10x10 nodes at (0,0) and (20,0) - not overlapping
        let node1 = LayoutNode::new("R1".to_string(), (10.0, 10.0)).with_position(0.0, 0.0);
        let node3 = LayoutNode::new("R3".to_string(), (10.0, 10.0)).with_position(20.0, 0.0);

        let result = node1.separation_from(&node3, 0.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_separation_from_with_padding() {
        // Two 10x10 nodes at (0,0) and (20,0) - not overlapping without padding
        // With padding 6.0, node1's bounds expand to (-11,-11) to (11,11)
        // That overlaps with node3's bounds (15,-5) to (25,5)
        let node1 = LayoutNode::new("R1".to_string(), (10.0, 10.0)).with_position(0.0, 0.0);
        let node3 = LayoutNode::new("R3".to_string(), (10.0, 10.0)).with_position(20.0, 0.0);

        // Without padding: no overlap
        assert!(node1.separation_from(&node3, 0.0).is_none());

        // With padding: overlap
        // node1 padded bounds: (-11,-11) to (11,11)
        // node3 bounds: (15,-5) to (25,5)
        // overlap_x = min(11,25) - max(-11,15) = 11 - 15 = -4 (no overlap still)
        // Actually need more padding
        let result = node1.separation_from(&node3, 10.0);
        // node1 padded bounds: (-15,-15) to (15,15)
        // node3 bounds: (15,-5) to (25,5)
        // overlap_x = min(15,25) - max(-15,15) = 15 - 15 = 0 (edge case)
        // Still no overlap, need even more
        assert!(result.is_none());

        let result = node1.separation_from(&node3, 11.0);
        // node1 padded bounds: (-16,-16) to (16,16)
        // node3 bounds: (15,-5) to (25,5)
        // overlap_x = min(16,25) - max(-16,15) = 16 - 15 = 1
        // overlap_y = min(16,5) - max(-16,-5) = 5 - (-5) = 10
        assert!(result.is_some());
        let (depth, dir) = result.unwrap();
        assert!((depth - 1.0).abs() < 0.001);
        assert!((dir.x - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_gap_from_separated() {
        // Two 10x10 nodes at (0,0) and (20,0) - gap of 10 units
        let node1 = LayoutNode::new("R1".to_string(), (10.0, 10.0)).with_position(0.0, 0.0);
        let node3 = LayoutNode::new("R3".to_string(), (10.0, 10.0)).with_position(20.0, 0.0);

        // node1 bounds: (-5,-5) to (5,5)
        // node3 bounds: (15,-5) to (25,5)
        // gap_x = 15 - 5 = 10 (separated horizontally)
        // gap_y = 0 (overlapping vertically)
        // Total gap = sqrt(10^2 + 0^2) = 10
        let gap = node1.gap_from(&node3);
        assert!((gap - 10.0).abs() < 0.001, "Expected gap 10.0, got {}", gap);
    }

    #[test]
    fn test_gap_from_overlapping() {
        // Two 10x10 nodes at (0,0) and (8,0) - overlapping
        let node1 = LayoutNode::new("R1".to_string(), (10.0, 10.0)).with_position(0.0, 0.0);
        let node2 = LayoutNode::new("R2".to_string(), (10.0, 10.0)).with_position(8.0, 0.0);

        let gap = node1.gap_from(&node2);
        assert!(gap.abs() < 0.001, "Expected gap 0.0 for overlapping, got {}", gap);
    }

    #[test]
    fn test_gap_from_diagonal() {
        // Two 10x10 nodes at (0,0) and (20,20) - separated diagonally
        let node1 = LayoutNode::new("R1".to_string(), (10.0, 10.0)).with_position(0.0, 0.0);
        let node4 = LayoutNode::new("R4".to_string(), (10.0, 10.0)).with_position(20.0, 20.0);

        // node1 bounds: (-5,-5) to (5,5)
        // node4 bounds: (15,15) to (25,25)
        // gap_x = 15 - 5 = 10
        // gap_y = 15 - 5 = 10
        // Total gap = sqrt(10^2 + 10^2) = sqrt(200) ≈ 14.14
        let gap = node1.gap_from(&node4);
        let expected = (200.0_f64).sqrt();
        assert!((gap - expected).abs() < 0.001, "Expected gap {}, got {}", expected, gap);
    }
}
