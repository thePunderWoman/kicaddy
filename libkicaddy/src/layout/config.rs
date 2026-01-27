//! Layout configuration types

use serde::{Deserialize, Serialize};

/// Configuration for force-directed layout algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// Number of iterations for the force simulation
    #[serde(default = "default_iterations")]
    pub iterations: usize,

    /// Initial temperature (controls maximum movement per iteration)
    #[serde(default = "default_temperature")]
    pub initial_temperature: f64,

    /// Cooling factor per iteration (temperature *= cooling_factor)
    #[serde(default = "default_cooling")]
    pub cooling_factor: f64,

    /// Ideal distance between connected nodes (in mm)
    #[serde(default = "default_ideal_distance")]
    pub ideal_distance: f64,

    /// Strength of repulsive forces between all nodes
    #[serde(default = "default_repulsion_strength")]
    pub repulsion_strength: f64,

    /// Strength of attractive forces along edges
    #[serde(default = "default_attraction_strength")]
    pub attraction_strength: f64,

    /// Strength of group affinity (attraction between nodes in same group)
    #[serde(default = "default_group_affinity")]
    pub group_affinity: f64,

    /// Strength of pin direction preference (bias for wire routing)
    #[serde(default = "default_pin_direction_strength")]
    pub pin_direction_strength: f64,

    /// Minimum distance between node bounding boxes (padding in mm)
    #[serde(default = "default_min_spacing")]
    pub min_spacing: f64,

    /// Starting X position for layout (in mm)
    #[serde(default = "default_start_x")]
    pub start_x: f64,

    /// Starting Y position for layout (in mm)
    #[serde(default = "default_start_y")]
    pub start_y: f64,

    /// Base strength of center attraction force (keeps layout on paper)
    #[serde(default = "default_center_attraction_base")]
    pub center_attraction_base: f64,

    /// Extra strength per mm outside paper bounds (soft boundary)
    #[serde(default = "default_center_attraction_overshoot")]
    pub center_attraction_overshoot: f64,

    /// Margin from paper edge (in mm) before overshoot force kicks in
    #[serde(default = "default_paper_margin")]
    pub paper_margin: f64,

    /// Strength of force to separate overlapping components
    #[serde(default = "default_overlap_separation_strength")]
    pub overlap_separation_strength: f64,

    /// Distance beyond which repulsion falls to zero (in mm)
    #[serde(default = "default_repulsion_falloff_distance")]
    pub repulsion_falloff_distance: f64,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            iterations: default_iterations(),
            initial_temperature: default_temperature(),
            cooling_factor: default_cooling(),
            ideal_distance: default_ideal_distance(),
            repulsion_strength: default_repulsion_strength(),
            attraction_strength: default_attraction_strength(),
            group_affinity: default_group_affinity(),
            pin_direction_strength: default_pin_direction_strength(),
            min_spacing: default_min_spacing(),
            start_x: default_start_x(),
            start_y: default_start_y(),
            center_attraction_base: default_center_attraction_base(),
            center_attraction_overshoot: default_center_attraction_overshoot(),
            paper_margin: default_paper_margin(),
            overlap_separation_strength: default_overlap_separation_strength(),
            repulsion_falloff_distance: default_repulsion_falloff_distance(),
        }
    }
}

fn default_iterations() -> usize {
    200
}

fn default_temperature() -> f64 {
    100.0 // mm - allow larger initial movements
}

fn default_cooling() -> f64 {
    0.97
}

fn default_ideal_distance() -> f64 {
    25.0 // mm - enough for typical component spacing
}

fn default_repulsion_strength() -> f64 {
    12.0 // Strong repulsion to prevent overlap
}

fn default_attraction_strength() -> f64 {
    0.5 // Weaker attraction so repulsion dominates for spacing
}

fn default_group_affinity() -> f64 {
    0.05 // Very gentle attraction - weaker than edge attraction
}

fn default_pin_direction_strength() -> f64 {
    0.8 // Strong bias for pin direction to avoid wire crossings
}

fn default_min_spacing() -> f64 {
    25.0 // mm padding between components - generous to avoid overlap
}

fn default_start_x() -> f64 {
    100.0 // mm from left edge
}

fn default_start_y() -> f64 {
    50.0 // mm from top edge
}

fn default_center_attraction_base() -> f64 {
    0.05 // Weak base attraction toward paper center
}

fn default_center_attraction_overshoot() -> f64 {
    0.5 // Strong pull per mm outside bounds
}

fn default_paper_margin() -> f64 {
    30.0 // mm margin from paper edge
}

fn default_overlap_separation_strength() -> f64 {
    50.0 // Very strong to quickly resolve overlaps
}

fn default_repulsion_falloff_distance() -> f64 {
    50.0 // mm - only repel nearby components
}

/// KiCAD schematic grid size in mm
pub const GRID_SIZE_MM: f64 = 1.27;

/// Snap a coordinate to the KiCAD grid
pub fn snap_to_grid(value: f64) -> f64 {
    (value / GRID_SIZE_MM).round() * GRID_SIZE_MM
}

/// Get paper dimensions (width, height) in mm for a given paper name
/// KiCAD uses landscape orientation (width > height for A-series)
pub fn paper_dimensions(paper: &str) -> (f64, f64) {
    match paper {
        "A4" => (297.0, 210.0),        // 297x210 mm (landscape)
        "A3" => (420.0, 297.0),        // 420x297 mm
        "A2" => (594.0, 420.0),        // 594x420 mm
        "A1" => (841.0, 594.0),        // 841x594 mm
        "A0" => (1189.0, 841.0),       // 1189x841 mm
        "A" => (279.4, 215.9),         // 11x8.5 inch
        "B" => (431.8, 279.4),         // 17x11 inch
        "C" => (558.8, 431.8),         // 22x17 inch
        "D" => (863.6, 558.8),         // 34x22 inch
        "E" => (1117.6, 863.6),        // 44x34 inch
        "USLetter" => (279.4, 215.9),  // 11x8.5 inch
        "USLegal" => (355.6, 215.9),   // 14x8.5 inch
        "USLedger" => (431.8, 279.4),  // 17x11 inch
        _ => (297.0, 210.0),           // Default to A4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snap_to_grid() {
        assert_eq!(snap_to_grid(0.0), 0.0);
        assert_eq!(snap_to_grid(1.27), 1.27);
        assert_eq!(snap_to_grid(1.0), 1.27); // rounds up
        assert_eq!(snap_to_grid(0.5), 0.0); // rounds down
        assert_eq!(snap_to_grid(2.54), 2.54);
        assert_eq!(snap_to_grid(2.0), 2.54); // rounds up
    }

    #[test]
    fn test_default_config() {
        let config = LayoutConfig::default();
        assert_eq!(config.iterations, 200);
        assert!(config.initial_temperature > 0.0);
        assert!(config.cooling_factor > 0.0 && config.cooling_factor < 1.0);
    }
}
