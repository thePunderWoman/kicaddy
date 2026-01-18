//! KiCAD symbol library parsing and types

pub mod graphics;
pub mod lookup;
pub mod pin;
pub mod types;

use std::path::Path;

use thiserror::Error;

use crate::parser::sexpr::{SExpr, SExprParser};

pub use graphics::*;
pub use pin::*;
pub use types::*;

#[derive(Debug, Error)]
pub enum SymbolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid format: expected {expected}, got {got}")]
    InvalidFormat { expected: String, got: String },
    #[error("Missing field: {0}")]
    MissingField(String),
}

/// Parse a symbol library from a file
pub fn parse_symbol_library(path: impl AsRef<Path>) -> Result<SymbolLibrary, SymbolError> {
    let content = std::fs::read_to_string(path)?;
    parse_symbol_library_str(&content)
}

/// Parse a symbol library from a string
pub fn parse_symbol_library_str(input: &str) -> Result<SymbolLibrary, SymbolError> {
    let (_, sexpr) = SExprParser::parse(input)
        .map_err(|e| SymbolError::Parse(format_parse_error(input, &e)))?;

    parse_library_sexpr(&sexpr)
}

/// Format a nom parse error with context and truncated input
fn format_parse_error(input: &str, e: &nom::Err<nom_language::error::VerboseError<&str>>) -> String {
    use crate::parser::sexpr::format_error;

    let full_error = format_error(input, e);

    // Truncate to avoid overwhelming output, but show enough context
    let lines: Vec<&str> = full_error.lines().collect();
    if lines.len() > 20 {
        let truncated: String = lines[..20].join("\n");
        format!("{}\n... ({} more lines)", truncated, lines.len() - 20)
    } else {
        full_error
    }
}

fn parse_library_sexpr(sexpr: &SExpr) -> Result<SymbolLibrary, SymbolError> {
    let items = sexpr.as_list_starting_with("kicad_symbol_lib")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "kicad_symbol_lib".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut version = 0;
    let mut generator = None;
    let mut generator_version = None;
    let mut symbols = Vec::new();

    for item in items {
        if let Some(args) = item.as_list_starting_with("version") {
            if let Some(v) = args.first().and_then(|e| e.as_number()) {
                version = v as u32;
            }
        } else if let Some(args) = item.as_list_starting_with("generator") {
            generator = args.first().and_then(|e| e.as_string()).map(|s| s.to_string());
        } else if let Some(args) = item.as_list_starting_with("generator_version") {
            generator_version = args.first().and_then(|e| e.as_string()).map(|s| s.to_string());
        } else if item.is_list_starting_with("symbol") {
            symbols.push(parse_symbol(item)?);
        }
    }

    Ok(SymbolLibrary {
        version,
        generator,
        generator_version,
        symbols,
    })
}

fn parse_symbol(sexpr: &SExpr) -> Result<Symbol, SymbolError> {
    let items = sexpr.as_list_starting_with("symbol")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "symbol".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let name = items.first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| SymbolError::MissingField("symbol name".to_string()))?
        .to_string();

    let mut pin_numbers_hide = false;
    let mut pin_names_offset = 0.0;
    let mut pin_names_hide = false;
    let mut exclude_from_sim = false;
    let mut in_bom = true;
    let mut on_board = true;
    let mut properties = Vec::new();
    let mut units = Vec::new();
    let mut embedded_fonts = None;

    for item in &items[1..] {
        if let Some(args) = item.as_list_starting_with("pin_numbers") {
            pin_numbers_hide = args.iter().any(|e| e.as_list_starting_with("hide").is_some());
            if pin_numbers_hide == false {
                // Check for (hide yes) pattern
                for arg in args {
                    if let Some(hide_args) = arg.as_list_starting_with("hide") {
                        if hide_args.first().and_then(|e| e.as_symbol()) == Some("yes") {
                            pin_numbers_hide = true;
                        }
                    }
                }
            }
        } else if let Some(args) = item.as_list_starting_with("pin_names") {
            for arg in args {
                if let Some(offset_args) = arg.as_list_starting_with("offset") {
                    if let Some(v) = offset_args.first().and_then(|e| e.as_number()) {
                        pin_names_offset = v;
                    }
                } else if arg.as_list_starting_with("hide").is_some() || arg.as_symbol() == Some("hide") {
                    pin_names_hide = true;
                }
            }
        } else if let Some(args) = item.as_list_starting_with("exclude_from_sim") {
            exclude_from_sim = args.first().and_then(|e| e.as_symbol()) == Some("yes");
        } else if let Some(args) = item.as_list_starting_with("in_bom") {
            in_bom = args.first().and_then(|e| e.as_symbol()) != Some("no");
        } else if let Some(args) = item.as_list_starting_with("on_board") {
            on_board = args.first().and_then(|e| e.as_symbol()) != Some("no");
        } else if item.is_list_starting_with("property") {
            properties.push(parse_property(item)?);
        } else if item.is_list_starting_with("symbol") {
            units.push(parse_symbol_unit(item)?);
        } else if let Some(args) = item.as_list_starting_with("embedded_fonts") {
            embedded_fonts = Some(args.first().and_then(|e| e.as_symbol()) != Some("no"));
        }
    }

    Ok(Symbol {
        name,
        pin_numbers_hide,
        pin_names_offset,
        pin_names_hide,
        exclude_from_sim,
        in_bom,
        on_board,
        properties,
        units,
        embedded_fonts,
    })
}

fn parse_property(sexpr: &SExpr) -> Result<Property, SymbolError> {
    let items = sexpr.as_list_starting_with("property")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "property".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    // Handle optional "private" keyword: (property private "name" "value" ...)
    let (_is_private, name_idx) = if items.first().and_then(|e| e.as_symbol()) == Some("private") {
        (true, 1)
    } else {
        (false, 0)
    };

    let name = items.get(name_idx)
        .and_then(|e| e.as_string())
        .ok_or_else(|| SymbolError::MissingField("property name".to_string()))?
        .to_string();

    let value = items.get(name_idx + 1)
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut position = None;
    let mut effects = None;

    for item in &items[(name_idx + 2)..] {
        if item.is_list_starting_with("at") {
            position = Some(parse_position(item)?);
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects(item)?);
        }
    }

    Ok(Property {
        name,
        value,
        position,
        effects,
    })
}

fn parse_position(sexpr: &SExpr) -> Result<Position, SymbolError> {
    let items = sexpr.as_list_starting_with("at")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "at".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let x = items.first().and_then(|e| e.as_number()).unwrap_or(0.0);
    let y = items.get(1).and_then(|e| e.as_number()).unwrap_or(0.0);
    let angle = items.get(2).and_then(|e| e.as_number()).unwrap_or(0.0);

    Ok(Position::new(x, y, angle))
}

fn parse_effects(sexpr: &SExpr) -> Result<Effects, SymbolError> {
    let items = sexpr.as_list_starting_with("effects")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "effects".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut font = None;
    let mut justify = None;
    let mut hide = false;

    for item in items {
        if item.is_list_starting_with("font") {
            font = Some(parse_font(item)?);
        } else if item.is_list_starting_with("justify") {
            justify = Some(parse_justify(item)?);
        } else if item.as_list_starting_with("hide").is_some() || item.as_symbol() == Some("hide") {
            hide = true;
        }
    }

    Ok(Effects { font, justify, hide })
}

fn parse_font(sexpr: &SExpr) -> Result<Font, SymbolError> {
    let items = sexpr.as_list_starting_with("font")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "font".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut size = None;
    let mut thickness = None;
    let mut bold = false;
    let mut italic = false;

    for item in items {
        if let Some(args) = item.as_list_starting_with("size") {
            let w = args.first().and_then(|e| e.as_number()).unwrap_or(1.27);
            let h = args.get(1).and_then(|e| e.as_number()).unwrap_or(1.27);
            size = Some((w, h));
        } else if let Some(args) = item.as_list_starting_with("thickness") {
            thickness = args.first().and_then(|e| e.as_number());
        } else if item.as_symbol() == Some("bold") {
            bold = true;
        } else if item.as_symbol() == Some("italic") {
            italic = true;
        }
    }

    Ok(Font { size, thickness, bold, italic })
}

fn parse_justify(sexpr: &SExpr) -> Result<Justify, SymbolError> {
    let items = sexpr.as_list_starting_with("justify")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "justify".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut horizontal = HorizontalJustify::Center;
    let mut vertical = VerticalJustify::Center;
    let mut mirror = false;

    for item in items {
        match item.as_symbol() {
            Some("left") => horizontal = HorizontalJustify::Left,
            Some("right") => horizontal = HorizontalJustify::Right,
            Some("top") => vertical = VerticalJustify::Top,
            Some("bottom") => vertical = VerticalJustify::Bottom,
            Some("mirror") => mirror = true,
            _ => {}
        }
    }

    Ok(Justify { horizontal, vertical, mirror })
}

fn parse_symbol_unit(sexpr: &SExpr) -> Result<SymbolUnit, SymbolError> {
    let items = sexpr.as_list_starting_with("symbol")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "symbol".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let name = items.first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| SymbolError::MissingField("unit name".to_string()))?
        .to_string();

    let mut graphics = Vec::new();
    let mut pins = Vec::new();

    for item in &items[1..] {
        if item.is_list_starting_with("rectangle") {
            graphics.push(GraphicItem::Rectangle(parse_rectangle(item)?));
        } else if item.is_list_starting_with("polyline") {
            graphics.push(GraphicItem::Polyline(parse_polyline(item)?));
        } else if item.is_list_starting_with("circle") {
            graphics.push(GraphicItem::Circle(parse_circle(item)?));
        } else if item.is_list_starting_with("arc") {
            graphics.push(GraphicItem::Arc(parse_arc(item)?));
        } else if item.is_list_starting_with("text") {
            graphics.push(GraphicItem::Text(parse_text(item)?));
        } else if item.is_list_starting_with("pin") {
            pins.push(parse_pin(item)?);
        }
    }

    Ok(SymbolUnit { name, graphics, pins })
}

fn parse_rectangle(sexpr: &SExpr) -> Result<Rectangle, SymbolError> {
    let items = sexpr.as_list_starting_with("rectangle")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "rectangle".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut start = Point::new(0.0, 0.0);
    let mut end = Point::new(0.0, 0.0);
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(args) = item.as_list_starting_with("start") {
            start = parse_point(args)?;
        } else if let Some(args) = item.as_list_starting_with("end") {
            end = parse_point(args)?;
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill(item)?;
        }
    }

    Ok(Rectangle { start, end, stroke, fill })
}

fn parse_polyline(sexpr: &SExpr) -> Result<Polyline, SymbolError> {
    let items = sexpr.as_list_starting_with("polyline")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "polyline".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut points = Vec::new();
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(pts_args) = item.as_list_starting_with("pts") {
            for pt in pts_args {
                if let Some(xy_args) = pt.as_list_starting_with("xy") {
                    points.push(parse_point(xy_args)?);
                }
            }
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill(item)?;
        }
    }

    Ok(Polyline { points, stroke, fill })
}

fn parse_circle(sexpr: &SExpr) -> Result<Circle, SymbolError> {
    let items = sexpr.as_list_starting_with("circle")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "circle".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut center = Point::new(0.0, 0.0);
    let mut radius = 0.0;
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(args) = item.as_list_starting_with("center") {
            center = parse_point(args)?;
        } else if let Some(args) = item.as_list_starting_with("radius") {
            radius = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill(item)?;
        }
    }

    Ok(Circle { center, radius, stroke, fill })
}

fn parse_arc(sexpr: &SExpr) -> Result<Arc, SymbolError> {
    let items = sexpr.as_list_starting_with("arc")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "arc".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut start = Point::new(0.0, 0.0);
    let mut mid = Point::new(0.0, 0.0);
    let mut end = Point::new(0.0, 0.0);
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(args) = item.as_list_starting_with("start") {
            start = parse_point(args)?;
        } else if let Some(args) = item.as_list_starting_with("mid") {
            mid = parse_point(args)?;
        } else if let Some(args) = item.as_list_starting_with("end") {
            end = parse_point(args)?;
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill(item)?;
        }
    }

    Ok(Arc { start, mid, end, stroke, fill })
}

fn parse_text(sexpr: &SExpr) -> Result<Text, SymbolError> {
    let items = sexpr.as_list_starting_with("text")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "text".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let text = items.first()
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut position = Position::new(0.0, 0.0, 0.0);
    let mut effects = None;

    for item in &items[1..] {
        if item.is_list_starting_with("at") {
            position = parse_position(item)?;
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects(item)?);
        }
    }

    Ok(Text { text, position, effects })
}

fn parse_pin(sexpr: &SExpr) -> Result<Pin, SymbolError> {
    let items = sexpr.as_list_starting_with("pin")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "pin".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    // First two items are electrical type and graphic style
    let electrical_type = items.first()
        .and_then(|e| e.as_symbol())
        .map(PinElectricalType::from_str)
        .unwrap_or(PinElectricalType::Unspecified);

    let graphic_style = items.get(1)
        .and_then(|e| e.as_symbol())
        .map(PinGraphicStyle::from_str)
        .unwrap_or(PinGraphicStyle::Line);

    let mut position = Position::new(0.0, 0.0, 0.0);
    let mut length = 0.0;
    let mut name = PinName { name: String::new(), effects: None };
    let mut number = PinNumber { number: String::new(), effects: None };
    let mut hide = false;

    for item in &items[2..] {
        if item.is_list_starting_with("at") {
            position = parse_position(item)?;
        } else if let Some(args) = item.as_list_starting_with("length") {
            length = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
        } else if item.is_list_starting_with("name") {
            name = parse_pin_name(item)?;
        } else if item.is_list_starting_with("number") {
            number = parse_pin_number(item)?;
        } else if item.as_symbol() == Some("hide") {
            hide = true;
        }
    }

    Ok(Pin {
        electrical_type,
        graphic_style,
        position,
        length,
        name,
        number,
        hide,
    })
}

fn parse_pin_name(sexpr: &SExpr) -> Result<PinName, SymbolError> {
    let items = sexpr.as_list_starting_with("name")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "name".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let name = items.first()
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut effects = None;
    for item in &items[1..] {
        if item.is_list_starting_with("effects") {
            effects = Some(parse_effects(item)?);
        }
    }

    Ok(PinName { name, effects })
}

fn parse_pin_number(sexpr: &SExpr) -> Result<PinNumber, SymbolError> {
    let items = sexpr.as_list_starting_with("number")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "number".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let number = items.first()
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut effects = None;
    for item in &items[1..] {
        if item.is_list_starting_with("effects") {
            effects = Some(parse_effects(item)?);
        }
    }

    Ok(PinNumber { number, effects })
}

fn parse_point(args: &[SExpr]) -> Result<Point, SymbolError> {
    let x = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
    let y = args.get(1).and_then(|e| e.as_number()).unwrap_or(0.0);
    Ok(Point::new(x, y))
}

fn parse_stroke(sexpr: &SExpr) -> Result<Stroke, SymbolError> {
    let items = sexpr.as_list_starting_with("stroke")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "stroke".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut width = 0.0;
    let mut stroke_type = StrokeType::Default;

    for item in items {
        if let Some(args) = item.as_list_starting_with("width") {
            width = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
        } else if let Some(args) = item.as_list_starting_with("type") {
            stroke_type = args.first()
                .and_then(|e| e.as_symbol())
                .map(StrokeType::from_str)
                .unwrap_or(StrokeType::Default);
        }
    }

    Ok(Stroke { width, stroke_type, color: None })
}

fn parse_fill(sexpr: &SExpr) -> Result<Fill, SymbolError> {
    let items = sexpr.as_list_starting_with("fill")
        .ok_or_else(|| SymbolError::InvalidFormat {
            expected: "fill".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut fill_type = FillType::None;

    for item in items {
        if let Some(args) = item.as_list_starting_with("type") {
            fill_type = args.first()
                .and_then(|e| e.as_symbol())
                .map(FillType::from_str)
                .unwrap_or(FillType::None);
        }
    }

    Ok(Fill { fill_type, color: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_symbol() {
        let input = r#"
            (kicad_symbol_lib
                (version 20241209)
                (generator "test")
                (symbol "R"
                    (pin_numbers (hide yes))
                    (pin_names (offset 0))
                    (exclude_from_sim no)
                    (in_bom yes)
                    (on_board yes)
                    (property "Reference" "R" (at 0 0 0))
                    (property "Value" "R" (at 0 0 0))
                    (symbol "R_0_1"
                        (rectangle
                            (start -1 -2.5)
                            (end 1 2.5)
                            (stroke (width 0.25) (type default))
                            (fill (type none))
                        )
                    )
                    (symbol "R_1_1"
                        (pin passive line
                            (at 0 3.81 270)
                            (length 1.27)
                            (name "~" (effects (font (size 1.27 1.27))))
                            (number "1" (effects (font (size 1.27 1.27))))
                        )
                    )
                )
            )
        "#;

        let lib = parse_symbol_library_str(input).unwrap();
        assert_eq!(lib.version, 20241209);
        assert_eq!(lib.symbols.len(), 1);

        let sym = &lib.symbols[0];
        assert_eq!(sym.name, "R");
        assert!(sym.pin_numbers_hide);
        assert_eq!(sym.reference(), Some("R"));
        assert_eq!(sym.units.len(), 2);

        // Check graphics
        assert_eq!(sym.units[0].graphics.len(), 1);
        assert!(matches!(sym.units[0].graphics[0], GraphicItem::Rectangle(_)));

        // Check pins
        assert_eq!(sym.units[1].pins.len(), 1);
        assert_eq!(sym.units[1].pins[0].number.number, "1");
    }
}
