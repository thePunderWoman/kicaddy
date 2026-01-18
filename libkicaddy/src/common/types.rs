//! Core data types shared across KiCAD file formats
//!
//! These types are used by both symbol libraries and schematics.

use serde::{Deserialize, Serialize};

/// Position with optional rotation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

impl Position {
    pub fn new(x: f64, y: f64, angle: f64) -> Self {
        Self { x, y, angle }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, angle: 0.0 }
    }
}

/// A 2D point
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Default for Point {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// Text effects (font, justification, visibility)
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Effects {
    pub font: Option<Font>,
    pub justify: Option<Justify>,
    pub hide: bool,
}

/// Font specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Font {
    pub size: Option<(f64, f64)>,
    pub thickness: Option<f64>,
    pub bold: bool,
    pub italic: bool,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            size: Some((1.27, 1.27)),
            thickness: None,
            bold: false,
            italic: false,
        }
    }
}

/// Text justification
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Justify {
    pub horizontal: HorizontalJustify,
    pub vertical: VerticalJustify,
    pub mirror: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum HorizontalJustify {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum VerticalJustify {
    Top,
    #[default]
    Center,
    Bottom,
}

/// A symbol/component property
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Property {
    /// Property name (Reference, Value, Footprint, etc.)
    pub name: String,
    /// Property value
    pub value: String,
    /// Position and rotation
    pub position: Option<Position>,
    /// Text effects
    pub effects: Option<Effects>,
}

/// Stroke style for lines and shapes
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Stroke {
    pub width: f64,
    pub stroke_type: StrokeType,
    pub color: Option<Color>,
}

/// Stroke line type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum StrokeType {
    #[default]
    Default,
    Solid,
    Dash,
    Dot,
    DashDot,
    DashDotDot,
}

impl StrokeType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "solid" => StrokeType::Solid,
            "dash" => StrokeType::Dash,
            "dot" => StrokeType::Dot,
            "dash_dot" => StrokeType::DashDot,
            "dash_dot_dot" => StrokeType::DashDotDot,
            _ => StrokeType::Default,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            StrokeType::Default => "default",
            StrokeType::Solid => "solid",
            StrokeType::Dash => "dash",
            StrokeType::Dot => "dot",
            StrokeType::DashDot => "dash_dot",
            StrokeType::DashDotDot => "dash_dot_dot",
        }
    }
}

/// Fill style for shapes
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Fill {
    pub fill_type: FillType,
    pub color: Option<Color>,
}

/// Fill type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum FillType {
    #[default]
    None,
    Outline,
    Background,
    Solid,
}

impl FillType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "outline" => FillType::Outline,
            "background" => FillType::Background,
            "solid" => FillType::Solid,
            _ => FillType::None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FillType::None => "none",
            FillType::Outline => "outline",
            FillType::Background => "background",
            FillType::Solid => "solid",
        }
    }
}

/// RGBA color
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}
