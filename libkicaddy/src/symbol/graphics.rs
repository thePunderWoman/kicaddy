//! Graphics elements for KiCAD symbols

use serde::{Deserialize, Serialize};

use crate::common::{Effects, Point, Position};

// Re-export common types for backwards compatibility
pub use crate::common::{Color, Fill, FillType, Stroke, StrokeType};

/// A graphic item in a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphicItem {
    Rectangle(Rectangle),
    Polyline(Polyline),
    Circle(Circle),
    Arc(Arc),
    Text(Text),
}

/// A rectangle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rectangle {
    pub start: Point,
    pub end: Point,
    pub stroke: Stroke,
    pub fill: Fill,
}

/// A polyline (series of connected points)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Polyline {
    pub points: Vec<Point>,
    pub stroke: Stroke,
    pub fill: Fill,
}

/// A circle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Circle {
    pub center: Point,
    pub radius: f64,
    pub stroke: Stroke,
    pub fill: Fill,
}

/// An arc defined by three points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arc {
    pub start: Point,
    pub mid: Point,
    pub end: Point,
    pub stroke: Stroke,
    pub fill: Fill,
}

/// Text element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
    pub position: Position,
    pub effects: Option<Effects>,
}
