//! Force-directed layout algorithm (Fruchterman-Reingold variant)

use crate::common::Point;

use super::config::{snap_to_grid, LayoutConfig};
use super::graph::{LayoutGraph, LayoutNode};

/// Force-directed layout algorithm
pub struct ForceDirectedLayout {
    config: LayoutConfig,
}

impl ForceDirectedLayout {
    /// Create a new layout instance with default configuration
    pub fn new() -> Self {
        Self {
            config: LayoutConfig::default(),
        }
    }

    /// Create a new layout instance with custom configuration
    pub fn with_config(config: LayoutConfig) -> Self {
        Self { config }
    }

    /// Run the layout algorithm on a graph
    pub fn layout(&self, graph: &mut LayoutGraph) {
        self.layout_for_paper(graph, 297.0, 210.0); // Default to A4
    }

    /// Run the layout algorithm and center on a paper of given size
    pub fn layout_for_paper(&self, graph: &mut LayoutGraph, paper_width: f64, paper_height: f64) {
        // Initialize positions for nodes without fixed positions
        self.initialize_positions(graph);

        // Calculate paper center for center attraction force
        let paper_center = Point::new(paper_width / 2.0, paper_height / 2.0);

        // Run the simulation
        let mut temperature = self.config.initial_temperature;

        for _iter in 0..self.config.iterations {
            // Calculate forces (includes center attraction)
            let forces = self.calculate_forces_with_center(graph, paper_center, paper_width, paper_height);

            // Apply forces (limited by temperature)
            self.apply_forces(graph, &forces, temperature);

            // Cool down
            temperature *= self.config.cooling_factor;
        }

        // Enforce minimum spacing as hard constraint
        self.enforce_minimum_spacing(graph);

        // Center the layout on the paper (for layouts that are within bounds)
        self.center_on_paper(graph, paper_width, paper_height);

        // Final pass: snap all positions to grid
        self.snap_all_to_grid(graph);
    }

    /// Initialize positions for nodes that don't have them
    fn initialize_positions(&self, graph: &mut LayoutGraph) {
        let node_count = graph.nodes.len();
        if node_count == 0 {
            return;
        }

        // Arrange nodes in a grid initially
        let cols = (node_count as f64).sqrt().ceil() as usize;
        let spacing = self.config.ideal_distance * 2.0;

        // Collect unfixed node references as owned strings to avoid borrow issues
        let mut unfixed_nodes: Vec<String> = graph
            .nodes
            .iter()
            .filter(|(_, n)| !n.fixed)
            .map(|(k, _)| k.clone())
            .collect();

        // Sort for deterministic layout
        unfixed_nodes.sort();

        for (i, reference) in unfixed_nodes.iter().enumerate() {
            let col = i % cols;
            let row = i / cols;

            if let Some(node) = graph.get_node_mut(reference) {
                node.position = Point::new(
                    self.config.start_x + col as f64 * spacing,
                    self.config.start_y + row as f64 * spacing,
                );
            }
        }
    }

    /// Calculate all forces acting on each node (without center attraction - for tests)
    #[allow(dead_code)]
    fn calculate_forces(&self, graph: &LayoutGraph) -> Vec<(String, Point)> {
        // Use A4 paper defaults for backward compatibility
        let paper_center = Point::new(297.0 / 2.0, 210.0 / 2.0);
        self.calculate_forces_with_center(graph, paper_center, 297.0, 210.0)
    }

    /// Calculate all forces acting on each node, including center attraction
    fn calculate_forces_with_center(
        &self,
        graph: &LayoutGraph,
        paper_center: Point,
        paper_width: f64,
        paper_height: f64,
    ) -> Vec<(String, Point)> {
        let mut forces: Vec<(String, Point)> = Vec::new();
        let k = self.config.ideal_distance;

        // Get references for iteration. `graph.nodes` is a HashMap, whose iteration order is
        // randomized per process — sorted here so the repulsive-force summation below happens in
        // the same order every run. Floating-point addition isn't strictly associative, so an
        // unsorted order can make the exact same yaml settle into a very slightly different final
        // position across separate `kicaddy compile` invocations, which occasionally lands on the
        // opposite side of a grid-snap boundary from the previous run — a real, if rare, source of
        // a compiled schematic changing between identical recompiles.
        let mut references: Vec<String> = graph.nodes.keys().cloned().collect();
        references.sort();

        for reference in &references {
            let node = match graph.get_node(reference) {
                Some(n) => n,
                None => continue,
            };

            if node.fixed {
                // Fixed nodes don't move
                forces.push((reference.clone(), Point::default()));
                continue;
            }

            let mut total_force = Point::default();

            // 1. Repulsive forces from all other nodes
            for other_ref in &references {
                if other_ref == reference {
                    continue;
                }

                let other = match graph.get_node(other_ref) {
                    Some(n) => n,
                    None => continue,
                };

                let force = self.repulsive_force(node, other, k);
                total_force.x += force.x;
                total_force.y += force.y;
            }

            // 2. Attractive forces along edges
            for edge in &graph.edges {
                let (other_ref, from_pin, to_pin) = if edge.from_node == *reference {
                    (&edge.to_node, &edge.from_pin, &edge.to_pin)
                } else if edge.to_node == *reference {
                    (&edge.from_node, &edge.to_pin, &edge.from_pin)
                } else {
                    continue;
                };

                let other = match graph.get_node(other_ref) {
                    Some(n) => n,
                    None => continue,
                };

                let force = self.attractive_force(node, other, from_pin, to_pin, k);
                total_force.x += force.x;
                total_force.y += force.y;
            }

            // 3. Group affinity force
            if let Some(ref group) = node.group {
                if let Some(centroid) = graph.group_centroid(Some(group)) {
                    let force = self.group_affinity_force(node, &centroid);
                    total_force.x += force.x;
                    total_force.y += force.y;
                }
            }

            // 4. Center attraction force (soft boundary)
            let center_force = self.center_attraction_force(node, paper_center, paper_width, paper_height);
            total_force.x += center_force.x;
            total_force.y += center_force.y;

            forces.push((reference.clone(), total_force));
        }

        forces
    }

    /// Calculate repulsive force between two nodes using AABB overlap detection
    fn repulsive_force(&self, node: &LayoutNode, other: &LayoutNode, k: f64) -> Point {
        // Check for AABB overlap (including min_spacing as padding)
        if let Some((overlap_depth, sep_dir)) =
            node.separation_from(other, self.config.min_spacing)
        {
            // OVERLAPPING: Apply strong separation force
            // Force scales with overlap depth to quickly resolve intersections
            let strength =
                self.config.overlap_separation_strength * k * (1.0 + overlap_depth / k);
            return Point::new(sep_dir.x * strength, sep_dir.y * strength);
        }

        // Not overlapping - check gap distance
        let gap = node.gap_from(other);

        if gap > self.config.repulsion_falloff_distance {
            // Far enough apart - no repulsion needed
            return Point::default();
        }

        // Within interaction range - gentle repulsion that falls off with distance
        let falloff = 1.0 - (gap / self.config.repulsion_falloff_distance);
        let strength = self.config.repulsion_strength * k * falloff * falloff;

        // Direction: away from other node's center
        let dx = node.position.x - other.position.x;
        let dy = node.position.y - other.position.y;
        let dist = (dx * dx + dy * dy).sqrt().max(0.1);

        Point::new(strength * dx / dist, strength * dy / dist)
    }

    /// Calculate attractive force along an edge (Hooke-like)
    fn attractive_force(
        &self,
        node: &LayoutNode,
        other: &LayoutNode,
        my_pin: &str,
        other_pin: &str,
        k: f64,
    ) -> Point {
        // Use pin-to-pin distance for accuracy
        let my_pos = node
            .pin_world_position(my_pin)
            .unwrap_or(node.position);
        let other_pos = other
            .pin_world_position(other_pin)
            .unwrap_or(other.position);

        let dx = other_pos.x - my_pos.x;
        let dy = other_pos.y - my_pos.y;
        let dist = (dx * dx + dy * dy).sqrt().max(0.1);

        // Fruchterman-Reingold attractive force: distance^2 / k
        let force_magnitude = (dist * dist / k) * self.config.attraction_strength;

        // Direction: toward the other node
        let norm_x = dx / dist;
        let norm_y = dy / dist;

        let mut force_x = force_magnitude * norm_x;
        let mut force_y = force_magnitude * norm_y;

        // Add pin direction preference bias from MY pin
        // Use constant force (scaled by k) so it doesn't weaken as nodes get closer
        if let Some(pin_info) = node.pins.get(my_pin) {
            let (pref_dx, pref_dy) = pin_info.wire_direction();

            // Check if the other node is in the preferred direction
            let dot = norm_x * pref_dx + norm_y * pref_dy;

            if dot < 0.0 {
                // Other node is in the wrong direction - add corrective force
                let correction = self.config.pin_direction_strength * k;
                force_x += pref_dx * correction;
                force_y += pref_dy * correction;
            }
        }

        // Add pin direction preference bias from OTHER pin (inverted - we want to be where their wire goes)
        if let Some(other_pin_info) = other.pins.get(other_pin) {
            let (other_pref_dx, other_pref_dy) = other_pin_info.wire_direction();

            // Check if we are in the direction the other pin's wire should go
            // norm points from us to them, so -norm points from them to us
            let dot = (-norm_x) * other_pref_dx + (-norm_y) * other_pref_dy;

            if dot < 0.0 {
                // We are on the wrong side of their pin - move toward their preferred wire direction
                let correction = self.config.pin_direction_strength * k;
                force_x += other_pref_dx * correction;
                force_y += other_pref_dy * correction;
            }
        }

        Point::new(force_x, force_y)
    }

    /// Calculate group affinity force (gentle pull toward group centroid)
    fn group_affinity_force(&self, node: &LayoutNode, centroid: &Point) -> Point {
        let dx = centroid.x - node.position.x;
        let dy = centroid.y - node.position.y;
        let dist = (dx * dx + dy * dy).sqrt().max(0.1);

        // Linear force toward centroid
        let force_magnitude = dist * self.config.group_affinity;

        let norm_x = dx / dist;
        let norm_y = dy / dist;

        Point::new(force_magnitude * norm_x, force_magnitude * norm_y)
    }

    /// Calculate center attraction force (soft boundary to keep layout on paper)
    ///
    /// This applies a gentle force toward the paper center that scales up
    /// progressively when outside paper bounds. This replaces hard clamping
    /// with a soft boundary that doesn't compress layouts unnaturally.
    fn center_attraction_force(
        &self,
        node: &LayoutNode,
        paper_center: Point,
        paper_width: f64,
        paper_height: f64,
    ) -> Point {
        let margin = self.config.paper_margin;

        // Direction toward center
        let dx = paper_center.x - node.position.x;
        let dy = paper_center.y - node.position.y;
        let dist = (dx * dx + dy * dy).sqrt().max(0.1);

        // Calculate how far outside the safe zone we are (0 if inside)
        let half_w = paper_width / 2.0 - margin;
        let half_h = paper_height / 2.0 - margin;

        let overshoot_x = ((node.position.x - paper_center.x).abs() - half_w).max(0.0);
        let overshoot_y = ((node.position.y - paper_center.y).abs() - half_h).max(0.0);
        let overshoot = (overshoot_x * overshoot_x + overshoot_y * overshoot_y).sqrt();

        // Base weak attraction + scaled-up force if outside bounds
        let strength = self.config.center_attraction_base
            + self.config.center_attraction_overshoot * overshoot;

        // Apply force toward center
        let norm_x = dx / dist;
        let norm_y = dy / dist;

        Point::new(strength * dist * norm_x, strength * dist * norm_y)
    }

    /// Apply forces to nodes (limited by temperature)
    fn apply_forces(
        &self,
        graph: &mut LayoutGraph,
        forces: &[(String, Point)],
        temperature: f64,
    ) {
        for (reference, force) in forces {
            if let Some(node) = graph.get_node_mut(reference) {
                if node.fixed {
                    continue;
                }

                // Limit displacement by temperature
                let force_mag = (force.x * force.x + force.y * force.y).sqrt();
                let limited_mag = force_mag.min(temperature);

                if force_mag > 0.001 {
                    let scale = limited_mag / force_mag;
                    node.position.x += force.x * scale;
                    node.position.y += force.y * scale;
                }
            }
        }
    }

    /// Snap all node positions to the grid
    fn snap_all_to_grid(&self, graph: &mut LayoutGraph) {
        for node in graph.nodes.values_mut() {
            node.position.x = snap_to_grid(node.position.x);
            node.position.y = snap_to_grid(node.position.y);
        }
    }

    /// Center the layout on the paper
    fn center_on_paper(&self, graph: &mut LayoutGraph, paper_width: f64, paper_height: f64) {
        if graph.nodes.is_empty() {
            return;
        }

        // Check if there are any fixed nodes - if so, don't center (respect user positions)
        let has_fixed = graph.nodes.values().any(|n| n.fixed);
        if has_fixed {
            return;
        }

        // Calculate bounding box of all nodes using just their center positions
        // (not including sizes, to avoid inflated bounds from large symbols)
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        for node in graph.nodes.values() {
            min_x = min_x.min(node.position.x);
            min_y = min_y.min(node.position.y);
            max_x = max_x.max(node.position.x);
            max_y = max_y.max(node.position.y);
        }

        // Calculate current center of the layout
        let layout_center_x = (min_x + max_x) / 2.0;
        let layout_center_y = (min_y + max_y) / 2.0;

        // Calculate paper center
        let paper_center_x = paper_width / 2.0;
        let paper_center_y = paper_height / 2.0;

        // Calculate offset needed to center
        let offset_x = paper_center_x - layout_center_x;
        let offset_y = paper_center_y - layout_center_y;

        // Apply offset to ALL nodes (move the entire layout together)
        for node in graph.nodes.values_mut() {
            node.position.x += offset_x;
            node.position.y += offset_y;
        }
    }

    /// Enforce minimum spacing between all node pairs as a hard constraint.
    /// This is a post-processing step to catch any spacing violations that
    /// the force simulation didn't fully resolve.
    fn enforce_minimum_spacing(&self, graph: &mut LayoutGraph) {
        let max_passes = 50;
        let min_spacing = self.config.min_spacing;

        for _pass in 0..max_passes {
            let mut any_violation = false;
            // Sorted for the same determinism reason as calculate_forces_with_center above —
            // which node in a violating pair gets pushed which way is order-sensitive.
            let mut refs: Vec<String> = graph.nodes.keys().cloned().collect();
            refs.sort();

            for i in 0..refs.len() {
                for j in (i + 1)..refs.len() {
                    // Skip if either node is fixed
                    let node_i_fixed = graph.get_node(&refs[i]).map(|n| n.fixed).unwrap_or(true);
                    let node_j_fixed = graph.get_node(&refs[j]).map(|n| n.fixed).unwrap_or(true);

                    if node_i_fixed && node_j_fixed {
                        continue;
                    }

                    // Check for spacing violation
                    let separation = {
                        let node_i = graph.get_node(&refs[i]).unwrap();
                        let node_j = graph.get_node(&refs[j]).unwrap();
                        node_i.separation_from(node_j, min_spacing)
                    };

                    if let Some((overlap_depth, sep_dir)) = separation {
                        any_violation = true;

                        // Push nodes apart - split the movement if both can move
                        let move_i = !node_i_fixed;
                        let move_j = !node_j_fixed;
                        let push_each = if move_i && move_j {
                            (overlap_depth / 2.0) + 0.1 // Add small buffer
                        } else {
                            overlap_depth + 0.1
                        };

                        // Apply movement
                        if move_i {
                            if let Some(node) = graph.get_node_mut(&refs[i]) {
                                node.position.x += sep_dir.x * push_each;
                                node.position.y += sep_dir.y * push_each;
                                node.position.x = snap_to_grid(node.position.x);
                                node.position.y = snap_to_grid(node.position.y);
                            }
                        }
                        if move_j {
                            if let Some(node) = graph.get_node_mut(&refs[j]) {
                                node.position.x -= sep_dir.x * push_each;
                                node.position.y -= sep_dir.y * push_each;
                                node.position.x = snap_to_grid(node.position.x);
                                node.position.y = snap_to_grid(node.position.y);
                            }
                        }
                    }
                }
            }

            if !any_violation {
                break;
            }
        }
    }
}

impl Default for ForceDirectedLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::graph::{LayoutEdge, PinInfo};

    fn make_test_node(reference: &str, x: f64, y: f64, fixed: bool) -> LayoutNode {
        let mut node = LayoutNode::new(reference.to_string(), (10.0, 5.0))
            .with_position(x, y)
            .with_fixed(fixed);

        // Add simple pins
        node.add_pin(
            "1".to_string(),
            PinInfo {
                offset: Point::new(-5.0, 0.0),
                direction: 180.0, // points left
                number: "1".to_string(),
                name: "".to_string(),
            },
        );
        node.add_pin(
            "2".to_string(),
            PinInfo {
                offset: Point::new(5.0, 0.0),
                direction: 0.0, // points right
                number: "2".to_string(),
                name: "".to_string(),
            },
        );

        node
    }

    #[test]
    fn test_two_connected_nodes_attract() {
        let mut graph = LayoutGraph::new();

        // Two nodes far apart
        graph.add_node(make_test_node("R1", 0.0, 0.0, false));
        graph.add_node(make_test_node("R2", 100.0, 0.0, false));

        // Connect them
        graph.add_edge(LayoutEdge::new(
            "R1".to_string(),
            "2".to_string(),
            "R2".to_string(),
            "1".to_string(),
        ));

        let layout = ForceDirectedLayout::new();
        layout.layout(&mut graph);

        // Nodes should be closer together
        let r1 = graph.get_node("R1").unwrap();
        let r2 = graph.get_node("R2").unwrap();
        let dist = ((r2.position.x - r1.position.x).powi(2)
            + (r2.position.y - r1.position.y).powi(2))
        .sqrt();

        assert!(
            dist < 100.0,
            "Connected nodes should be closer: {}",
            dist
        );
    }

    #[test]
    fn test_unconnected_nodes_repel() {
        let mut graph = LayoutGraph::new();

        // Two nodes close together but not connected
        graph.add_node(make_test_node("R1", 50.0, 50.0, false));
        graph.add_node(make_test_node("R2", 52.0, 50.0, false));

        let layout = ForceDirectedLayout::new();
        layout.layout(&mut graph);

        // Nodes should be pushed apart
        let r1 = graph.get_node("R1").unwrap();
        let r2 = graph.get_node("R2").unwrap();
        let dist = ((r2.position.x - r1.position.x).powi(2)
            + (r2.position.y - r1.position.y).powi(2))
        .sqrt();

        assert!(
            dist > 5.0,
            "Unconnected nodes should be pushed apart: {}",
            dist
        );
    }

    #[test]
    fn test_fixed_nodes_dont_move() {
        let mut graph = LayoutGraph::new();

        // Fixed node - use grid-aligned position (100.33 = 79 * 1.27)
        graph.add_node(make_test_node("R1", 100.33, 100.33, true));
        // Unfixed node very close
        graph.add_node(make_test_node("R2", 101.6, 100.33, false));

        let layout = ForceDirectedLayout::new();
        layout.layout(&mut graph);

        // R1 should not have moved (already on grid)
        let r1 = graph.get_node("R1").unwrap();
        assert!((r1.position.x - 100.33).abs() < 0.01, "R1 X moved: {}", r1.position.x);
        assert!((r1.position.y - 100.33).abs() < 0.01, "R1 Y moved: {}", r1.position.y);
    }

    #[test]
    fn test_group_affinity() {
        let mut graph = LayoutGraph::new();

        // Three nodes in the same group, starting far apart
        graph.add_node(
            make_test_node("R1", 0.0, 0.0, false).with_group(Some("Power".to_string())),
        );
        graph.add_node(
            make_test_node("R2", 200.0, 0.0, false).with_group(Some("Power".to_string())),
        );
        graph.add_node(
            make_test_node("R3", 100.0, 200.0, false).with_group(Some("Power".to_string())),
        );

        // One node in a different group
        graph.add_node(
            make_test_node("U1", 100.0, 100.0, false).with_group(Some("Logic".to_string())),
        );

        let layout = ForceDirectedLayout::new();
        layout.layout(&mut graph);

        // The Power group nodes should be clustered
        let r1 = graph.get_node("R1").unwrap();
        let r2 = graph.get_node("R2").unwrap();
        let r3 = graph.get_node("R3").unwrap();

        // Calculate centroid
        let centroid_x = (r1.position.x + r2.position.x + r3.position.x) / 3.0;
        let centroid_y = (r1.position.y + r2.position.y + r3.position.y) / 3.0;

        // Average distance to centroid should be reasonable
        let avg_dist = ((r1.position.x - centroid_x).powi(2)
            + (r1.position.y - centroid_y).powi(2))
        .sqrt()
            + ((r2.position.x - centroid_x).powi(2) + (r2.position.y - centroid_y).powi(2)).sqrt()
            + ((r3.position.x - centroid_x).powi(2) + (r3.position.y - centroid_y).powi(2)).sqrt();
        let avg_dist = avg_dist / 3.0;

        // Should be clustered within reasonable distance
        assert!(
            avg_dist < 150.0,
            "Group nodes should be clustered: avg dist = {}",
            avg_dist
        );
    }

    #[test]
    fn test_positions_snap_to_grid() {
        let mut graph = LayoutGraph::new();

        graph.add_node(make_test_node("R1", 50.5, 30.3, false));

        let layout = ForceDirectedLayout::new();
        layout.layout(&mut graph);

        let r1 = graph.get_node("R1").unwrap();

        // Position should be on grid (multiple of 1.27)
        let x_remainder = r1.position.x % 1.27;
        let y_remainder = r1.position.y % 1.27;

        assert!(
            x_remainder.abs() < 0.001 || (1.27 - x_remainder).abs() < 0.001,
            "X not on grid: {} (remainder {})",
            r1.position.x,
            x_remainder
        );
        assert!(
            y_remainder.abs() < 0.001 || (1.27 - y_remainder).abs() < 0.001,
            "Y not on grid: {} (remainder {})",
            r1.position.y,
            y_remainder
        );
    }

    #[test]
    fn test_three_node_chain() {
        let mut graph = LayoutGraph::new();

        // Three nodes in a chain: R1 -- R2 -- R3
        graph.add_node(make_test_node("R1", 0.0, 0.0, false));
        graph.add_node(make_test_node("R2", 100.0, 50.0, false));
        graph.add_node(make_test_node("R3", 200.0, 0.0, false));

        graph.add_edge(LayoutEdge::new(
            "R1".to_string(),
            "2".to_string(),
            "R2".to_string(),
            "1".to_string(),
        ));
        graph.add_edge(LayoutEdge::new(
            "R2".to_string(),
            "2".to_string(),
            "R3".to_string(),
            "1".to_string(),
        ));

        let layout = ForceDirectedLayout::new();
        layout.layout(&mut graph);

        // R2 should be between R1 and R3
        let r1 = graph.get_node("R1").unwrap();
        let r2 = graph.get_node("R2").unwrap();
        let r3 = graph.get_node("R3").unwrap();

        let d12 = ((r2.position.x - r1.position.x).powi(2)
            + (r2.position.y - r1.position.y).powi(2))
        .sqrt();
        let d23 = ((r3.position.x - r2.position.x).powi(2)
            + (r3.position.y - r2.position.y).powi(2))
        .sqrt();
        let d13 = ((r3.position.x - r1.position.x).powi(2)
            + (r3.position.y - r1.position.y).powi(2))
        .sqrt();

        // R2 should be roughly between R1 and R3 (d12 + d23 should be close to d13)
        // Allow some tolerance for the force-directed nature
        assert!(
            d12 + d23 < d13 * 1.5,
            "R2 should be between R1 and R3: d12={}, d23={}, d13={}",
            d12,
            d23,
            d13
        );
    }

    #[test]
    fn test_minimum_spacing_enforced() {
        use crate::layout::config::LayoutConfig;

        let mut graph = LayoutGraph::new();

        // Create a custom config with specific min_spacing for testing
        let config = LayoutConfig {
            min_spacing: 15.0, // 15mm minimum gap
            ..Default::default()
        };

        // Two nodes placed very close together (overlapping considering size + spacing)
        // Each node is 10mm wide, so with 15mm spacing requirement,
        // they need 10/2 + 15 + 10/2 = 25mm center-to-center minimum
        graph.add_node(make_test_node("C1", 100.0, 100.0, false));
        graph.add_node(make_test_node("C2", 105.0, 100.0, false)); // Only 5mm apart

        let layout = ForceDirectedLayout::with_config(config);
        layout.layout(&mut graph);

        let c1 = graph.get_node("C1").unwrap();
        let c2 = graph.get_node("C2").unwrap();

        // Calculate the gap between the two nodes
        let gap = c1.gap_from(c2);

        // Gap should be at least min_spacing (15mm), allowing small tolerance for grid snapping
        assert!(
            gap >= 14.0,
            "Minimum spacing not enforced: gap = {}mm (expected >= 15mm)",
            gap
        );
    }

    #[test]
    fn test_minimum_spacing_with_fixed_node() {
        use crate::layout::config::LayoutConfig;

        let mut graph = LayoutGraph::new();

        let config = LayoutConfig {
            min_spacing: 15.0,
            ..Default::default()
        };

        // Fixed node at a specific position (grid-aligned)
        graph.add_node(make_test_node("C1", 100.33, 100.33, true)); // Fixed
        graph.add_node(make_test_node("C2", 105.41, 100.33, false)); // Movable, too close

        let layout = ForceDirectedLayout::with_config(config);
        layout.layout(&mut graph);

        let c1 = graph.get_node("C1").unwrap();
        let c2 = graph.get_node("C2").unwrap();

        // C1 should not have moved (it's fixed)
        assert!(
            (c1.position.x - 100.33).abs() < 0.01,
            "Fixed node C1 moved: {}",
            c1.position.x
        );

        // Gap should still be enforced (C2 moved away)
        let gap = c1.gap_from(c2);
        assert!(
            gap >= 14.0,
            "Minimum spacing not enforced with fixed node: gap = {}mm",
            gap
        );
    }
}
