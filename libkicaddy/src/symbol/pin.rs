//! Pin types for KiCAD symbols

use serde::{Deserialize, Serialize};

use crate::common::{Effects, Position};

/// A component pin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin {
    /// Electrical type
    pub electrical_type: PinElectricalType,
    /// Graphical style
    pub graphic_style: PinGraphicStyle,
    /// Position and rotation
    pub position: Position,
    /// Pin length
    pub length: f64,
    /// Pin name
    pub name: PinName,
    /// Pin number
    pub number: PinNumber,
    /// Whether the pin is hidden
    pub hide: bool,
}

/// Pin name with optional effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinName {
    pub name: String,
    pub effects: Option<Effects>,
}

/// Pin number with optional effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinNumber {
    pub number: String,
    pub effects: Option<Effects>,
}

/// Pin electrical type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PinElectricalType {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
    Free,
    Unspecified,
    PowerIn,
    PowerOut,
    OpenCollector,
    OpenEmitter,
    NoConnect,
}

impl PinElectricalType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "input" => PinElectricalType::Input,
            "output" => PinElectricalType::Output,
            "bidirectional" => PinElectricalType::Bidirectional,
            "tri_state" => PinElectricalType::TriState,
            "passive" => PinElectricalType::Passive,
            "free" => PinElectricalType::Free,
            "unspecified" => PinElectricalType::Unspecified,
            "power_in" => PinElectricalType::PowerIn,
            "power_out" => PinElectricalType::PowerOut,
            "open_collector" => PinElectricalType::OpenCollector,
            "open_emitter" => PinElectricalType::OpenEmitter,
            "no_connect" => PinElectricalType::NoConnect,
            _ => PinElectricalType::Unspecified,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PinElectricalType::Input => "input",
            PinElectricalType::Output => "output",
            PinElectricalType::Bidirectional => "bidirectional",
            PinElectricalType::TriState => "tri_state",
            PinElectricalType::Passive => "passive",
            PinElectricalType::Free => "free",
            PinElectricalType::Unspecified => "unspecified",
            PinElectricalType::PowerIn => "power_in",
            PinElectricalType::PowerOut => "power_out",
            PinElectricalType::OpenCollector => "open_collector",
            PinElectricalType::OpenEmitter => "open_emitter",
            PinElectricalType::NoConnect => "no_connect",
        }
    }
}

/// Pin graphic style
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PinGraphicStyle {
    Line,
    Inverted,
    Clock,
    InvertedClock,
    InputLow,
    ClockLow,
    OutputLow,
    EdgeClockHigh,
    NonLogic,
}

impl PinGraphicStyle {
    pub fn from_str(s: &str) -> Self {
        match s {
            "line" => PinGraphicStyle::Line,
            "inverted" => PinGraphicStyle::Inverted,
            "clock" => PinGraphicStyle::Clock,
            "inverted_clock" => PinGraphicStyle::InvertedClock,
            "input_low" => PinGraphicStyle::InputLow,
            "clock_low" => PinGraphicStyle::ClockLow,
            "output_low" => PinGraphicStyle::OutputLow,
            "edge_clock_high" => PinGraphicStyle::EdgeClockHigh,
            "non_logic" => PinGraphicStyle::NonLogic,
            _ => PinGraphicStyle::Line,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PinGraphicStyle::Line => "line",
            PinGraphicStyle::Inverted => "inverted",
            PinGraphicStyle::Clock => "clock",
            PinGraphicStyle::InvertedClock => "inverted_clock",
            PinGraphicStyle::InputLow => "input_low",
            PinGraphicStyle::ClockLow => "clock_low",
            PinGraphicStyle::OutputLow => "output_low",
            PinGraphicStyle::EdgeClockHigh => "edge_clock_high",
            PinGraphicStyle::NonLogic => "non_logic",
        }
    }
}
