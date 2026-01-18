//! KiCAD schematic parsing and types

pub mod serialize;
pub mod types;

use std::path::Path;

use thiserror::Error;

use crate::common::{
    Color, Effects, Fill, FillType, Font, HorizontalJustify, Justify, Point, Position, Property,
    Stroke, StrokeType, VerticalJustify,
};
use crate::parser::sexpr::{SExpr, SExprParser};
use crate::symbol::{
    GraphicItem, Pin, PinElectricalType, PinGraphicStyle, PinName, PinNumber, Symbol, SymbolUnit,
};

pub use types::*;

#[derive(Debug, Error)]
pub enum SchematicError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid format: expected {expected}, got {got}")]
    InvalidFormat { expected: String, got: String },
    #[error("Missing field: {0}")]
    MissingField(String),
}

/// Parse a schematic from a file
pub fn parse_schematic(path: impl AsRef<Path>) -> Result<Schematic, SchematicError> {
    let content = std::fs::read_to_string(path)?;
    parse_schematic_str(&content)
}

/// Parse a schematic from a string
pub fn parse_schematic_str(input: &str) -> Result<Schematic, SchematicError> {
    let (_, sexpr) = SExprParser::parse(input)
        .map_err(|e| SchematicError::Parse(format_parse_error(input, &e)))?;

    parse_schematic_sexpr(&sexpr)
}

/// Format a nom parse error with context and truncated input
fn format_parse_error(
    input: &str,
    e: &nom::Err<nom_language::error::VerboseError<&str>>,
) -> String {
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

fn parse_schematic_sexpr(sexpr: &SExpr) -> Result<Schematic, SchematicError> {
    let items = sexpr.as_list_starting_with("kicad_sch").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "kicad_sch".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut schematic = Schematic::new();

    for item in items {
        if let Some(args) = item.as_list_starting_with("version") {
            if let Some(v) = args.first().and_then(|e| e.as_number()) {
                schematic.version = v as u32;
            }
        } else if let Some(args) = item.as_list_starting_with("generator") {
            schematic.generator = args
                .first()
                .and_then(|e| e.as_string())
                .map(|s| s.to_string());
        } else if let Some(args) = item.as_list_starting_with("generator_version") {
            schematic.generator_version = args
                .first()
                .and_then(|e| e.as_string())
                .map(|s| s.to_string());
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            schematic.uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        } else if let Some(args) = item.as_list_starting_with("paper") {
            if let Some(size_str) = args.first().and_then(|e| e.as_string()) {
                schematic.paper = PaperSize::from_str(size_str);
            }
        } else if item.is_list_starting_with("title_block") {
            schematic.title_block = Some(parse_title_block(item)?);
        } else if item.is_list_starting_with("lib_symbols") {
            schematic.lib_symbols = parse_lib_symbols(item)?;
        } else if item.is_list_starting_with("junction") {
            schematic.junctions.push(parse_junction(item)?);
        } else if item.is_list_starting_with("no_connect") {
            schematic.no_connects.push(parse_no_connect(item)?);
        } else if item.is_list_starting_with("wire") {
            schematic.wires.push(parse_wire(item)?);
        } else if item.is_list_starting_with("bus") {
            schematic.buses.push(parse_bus(item)?);
        } else if item.is_list_starting_with("bus_entry") {
            schematic.bus_entries.push(parse_bus_entry(item)?);
        } else if item.is_list_starting_with("global_label") {
            schematic.global_labels.push(parse_global_label(item)?);
        } else if item.is_list_starting_with("hierarchical_label") {
            schematic
                .hierarchical_labels
                .push(parse_hierarchical_label(item)?);
        } else if item.is_list_starting_with("label") {
            schematic.labels.push(parse_label(item)?);
        } else if item.is_list_starting_with("text") {
            schematic.text_items.push(parse_text_item(item)?);
        } else if item.is_list_starting_with("symbol") {
            schematic.symbols.push(parse_symbol_instance(item)?);
        } else if item.is_list_starting_with("sheet_instances") {
            schematic.sheet_instances = parse_sheet_instances(item)?;
        } else if let Some(args) = item.as_list_starting_with("embedded_fonts") {
            schematic.embedded_fonts = args.first().and_then(|e| e.as_symbol()) == Some("yes");
        }
    }

    Ok(schematic)
}

fn parse_title_block(sexpr: &SExpr) -> Result<TitleBlock, SchematicError> {
    let items = sexpr.as_list_starting_with("title_block").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "title_block".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut title_block = TitleBlock::default();

    for item in items {
        if let Some(args) = item.as_list_starting_with("title") {
            title_block.title = args.first().and_then(|e| e.as_string()).map(|s| s.to_string());
        } else if let Some(args) = item.as_list_starting_with("date") {
            title_block.date = args.first().and_then(|e| e.as_string()).map(|s| s.to_string());
        } else if let Some(args) = item.as_list_starting_with("rev") {
            title_block.rev = args.first().and_then(|e| e.as_string()).map(|s| s.to_string());
        } else if let Some(args) = item.as_list_starting_with("company") {
            title_block.company = args.first().and_then(|e| e.as_string()).map(|s| s.to_string());
        } else if let Some(args) = item.as_list_starting_with("comment") {
            if let (Some(idx), Some(text)) = (
                args.first().and_then(|e| e.as_number()),
                args.get(1).and_then(|e| e.as_string()),
            ) {
                title_block.comments.push((idx as u8, text.to_string()));
            }
        }
    }

    Ok(title_block)
}

fn parse_lib_symbols(sexpr: &SExpr) -> Result<Vec<Symbol>, SchematicError> {
    let items = sexpr.as_list_starting_with("lib_symbols").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "lib_symbols".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut symbols = Vec::new();
    for item in items {
        if item.is_list_starting_with("symbol") {
            // Reuse the symbol parser from the symbol module
            let symbol =
                parse_symbol_from_schematic(item).map_err(|e| SchematicError::Parse(e))?;
            symbols.push(symbol);
        }
    }

    Ok(symbols)
}

/// Parse a symbol definition from a schematic file
/// This is similar to the library symbol parser but handles schematic-specific cases
fn parse_symbol_from_schematic(sexpr: &SExpr) -> Result<Symbol, String> {
    let items = sexpr
        .as_list_starting_with("symbol")
        .ok_or_else(|| format!("Expected symbol, got {:?}", sexpr))?;

    let name = items
        .first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| "Missing symbol name".to_string())?
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
            for arg in args {
                if let Some(hide_args) = arg.as_list_starting_with("hide") {
                    if hide_args.first().and_then(|e| e.as_symbol()) == Some("yes") {
                        pin_numbers_hide = true;
                    }
                }
            }
        } else if let Some(args) = item.as_list_starting_with("pin_names") {
            for arg in args {
                if let Some(offset_args) = arg.as_list_starting_with("offset") {
                    if let Some(v) = offset_args.first().and_then(|e| e.as_number()) {
                        pin_names_offset = v;
                    }
                } else if arg.as_list_starting_with("hide").is_some()
                    || arg.as_symbol() == Some("hide")
                {
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
            properties.push(parse_property_inner(item)?);
        } else if item.is_list_starting_with("symbol") {
            units.push(parse_symbol_unit_inner(item)?);
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

fn parse_property_inner(sexpr: &SExpr) -> Result<Property, String> {
    let items = sexpr
        .as_list_starting_with("property")
        .ok_or_else(|| format!("Expected property, got {:?}", sexpr))?;

    // Handle optional "private" keyword: (property private "name" "value" ...)
    let (_is_private, name_idx) = if items.first().and_then(|e| e.as_symbol()) == Some("private") {
        (true, 1)
    } else {
        (false, 0)
    };

    let name = items
        .get(name_idx)
        .and_then(|e| e.as_string())
        .ok_or_else(|| "Missing property name".to_string())?
        .to_string();

    let value = items
        .get(name_idx + 1)
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut position = None;
    let mut effects = None;

    for item in &items[(name_idx + 2)..] {
        if item.is_list_starting_with("at") {
            position = Some(parse_position_inner(item)?);
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item)?);
        }
    }

    Ok(Property {
        name,
        value,
        position,
        effects,
    })
}

fn parse_position_inner(sexpr: &SExpr) -> Result<Position, String> {
    let items = sexpr
        .as_list_starting_with("at")
        .ok_or_else(|| format!("Expected at, got {:?}", sexpr))?;

    let x = items.first().and_then(|e| e.as_number()).unwrap_or(0.0);
    let y = items.get(1).and_then(|e| e.as_number()).unwrap_or(0.0);
    let angle = items.get(2).and_then(|e| e.as_number()).unwrap_or(0.0);

    Ok(Position::new(x, y, angle))
}

fn parse_effects_inner(sexpr: &SExpr) -> Result<Effects, String> {
    let items = sexpr
        .as_list_starting_with("effects")
        .ok_or_else(|| format!("Expected effects, got {:?}", sexpr))?;

    let mut font = None;
    let mut justify = None;
    let mut hide = false;

    for item in items {
        if item.is_list_starting_with("font") {
            font = Some(parse_font_inner(item)?);
        } else if item.is_list_starting_with("justify") {
            justify = Some(parse_justify_inner(item)?);
        } else if item.as_list_starting_with("hide").is_some() || item.as_symbol() == Some("hide") {
            hide = true;
        }
    }

    Ok(Effects { font, justify, hide })
}

fn parse_font_inner(sexpr: &SExpr) -> Result<Font, String> {
    let items = sexpr
        .as_list_starting_with("font")
        .ok_or_else(|| format!("Expected font, got {:?}", sexpr))?;

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

    Ok(Font {
        size,
        thickness,
        bold,
        italic,
    })
}

fn parse_justify_inner(sexpr: &SExpr) -> Result<Justify, String> {
    let items = sexpr
        .as_list_starting_with("justify")
        .ok_or_else(|| format!("Expected justify, got {:?}", sexpr))?;

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

    Ok(Justify {
        horizontal,
        vertical,
        mirror,
    })
}

fn parse_symbol_unit_inner(sexpr: &SExpr) -> Result<SymbolUnit, String> {
    let items = sexpr
        .as_list_starting_with("symbol")
        .ok_or_else(|| format!("Expected symbol, got {:?}", sexpr))?;

    let name = items
        .first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| "Missing unit name".to_string())?
        .to_string();

    let mut graphics = Vec::new();
    let mut pins = Vec::new();

    for item in &items[1..] {
        if item.is_list_starting_with("rectangle") {
            graphics.push(GraphicItem::Rectangle(parse_rectangle_inner(item)?));
        } else if item.is_list_starting_with("polyline") {
            graphics.push(GraphicItem::Polyline(parse_polyline_inner(item)?));
        } else if item.is_list_starting_with("circle") {
            graphics.push(GraphicItem::Circle(parse_circle_inner(item)?));
        } else if item.is_list_starting_with("arc") {
            graphics.push(GraphicItem::Arc(parse_arc_inner(item)?));
        } else if item.is_list_starting_with("text") {
            graphics.push(GraphicItem::Text(parse_text_inner(item)?));
        } else if item.is_list_starting_with("pin") {
            pins.push(parse_pin_inner(item)?);
        }
    }

    Ok(SymbolUnit { name, graphics, pins })
}

fn parse_rectangle_inner(
    sexpr: &SExpr,
) -> Result<crate::symbol::graphics::Rectangle, String> {
    let items = sexpr
        .as_list_starting_with("rectangle")
        .ok_or_else(|| format!("Expected rectangle, got {:?}", sexpr))?;

    let mut start = Point::new(0.0, 0.0);
    let mut end = Point::new(0.0, 0.0);
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(args) = item.as_list_starting_with("start") {
            start = parse_point_inner(args)?;
        } else if let Some(args) = item.as_list_starting_with("end") {
            end = parse_point_inner(args)?;
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke_inner(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill_inner(item)?;
        }
    }

    Ok(crate::symbol::graphics::Rectangle {
        start,
        end,
        stroke,
        fill,
    })
}

fn parse_polyline_inner(sexpr: &SExpr) -> Result<crate::symbol::graphics::Polyline, String> {
    let items = sexpr
        .as_list_starting_with("polyline")
        .ok_or_else(|| format!("Expected polyline, got {:?}", sexpr))?;

    let mut points = Vec::new();
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(pts_args) = item.as_list_starting_with("pts") {
            for pt in pts_args {
                if let Some(xy_args) = pt.as_list_starting_with("xy") {
                    points.push(parse_point_inner(xy_args)?);
                }
            }
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke_inner(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill_inner(item)?;
        }
    }

    Ok(crate::symbol::graphics::Polyline {
        points,
        stroke,
        fill,
    })
}

fn parse_circle_inner(sexpr: &SExpr) -> Result<crate::symbol::graphics::Circle, String> {
    let items = sexpr
        .as_list_starting_with("circle")
        .ok_or_else(|| format!("Expected circle, got {:?}", sexpr))?;

    let mut center = Point::new(0.0, 0.0);
    let mut radius = 0.0;
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(args) = item.as_list_starting_with("center") {
            center = parse_point_inner(args)?;
        } else if let Some(args) = item.as_list_starting_with("radius") {
            radius = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke_inner(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill_inner(item)?;
        }
    }

    Ok(crate::symbol::graphics::Circle {
        center,
        radius,
        stroke,
        fill,
    })
}

fn parse_arc_inner(sexpr: &SExpr) -> Result<crate::symbol::graphics::Arc, String> {
    let items = sexpr
        .as_list_starting_with("arc")
        .ok_or_else(|| format!("Expected arc, got {:?}", sexpr))?;

    let mut start = Point::new(0.0, 0.0);
    let mut mid = Point::new(0.0, 0.0);
    let mut end = Point::new(0.0, 0.0);
    let mut stroke = Stroke::default();
    let mut fill = Fill::default();

    for item in items {
        if let Some(args) = item.as_list_starting_with("start") {
            start = parse_point_inner(args)?;
        } else if let Some(args) = item.as_list_starting_with("mid") {
            mid = parse_point_inner(args)?;
        } else if let Some(args) = item.as_list_starting_with("end") {
            end = parse_point_inner(args)?;
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke_inner(item)?;
        } else if item.is_list_starting_with("fill") {
            fill = parse_fill_inner(item)?;
        }
    }

    Ok(crate::symbol::graphics::Arc {
        start,
        mid,
        end,
        stroke,
        fill,
    })
}

fn parse_text_inner(sexpr: &SExpr) -> Result<crate::symbol::graphics::Text, String> {
    let items = sexpr
        .as_list_starting_with("text")
        .ok_or_else(|| format!("Expected text, got {:?}", sexpr))?;

    let text = items
        .first()
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut position = Position::new(0.0, 0.0, 0.0);
    let mut effects = None;

    for item in &items[1..] {
        if item.is_list_starting_with("at") {
            position = parse_position_inner(item)?;
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item)?);
        }
    }

    Ok(crate::symbol::graphics::Text {
        text,
        position,
        effects,
    })
}

fn parse_pin_inner(sexpr: &SExpr) -> Result<Pin, String> {
    let items = sexpr
        .as_list_starting_with("pin")
        .ok_or_else(|| format!("Expected pin, got {:?}", sexpr))?;

    // First two items are electrical type and graphic style
    let electrical_type = items
        .first()
        .and_then(|e| e.as_symbol())
        .map(PinElectricalType::from_str)
        .unwrap_or(PinElectricalType::Unspecified);

    let graphic_style = items
        .get(1)
        .and_then(|e| e.as_symbol())
        .map(PinGraphicStyle::from_str)
        .unwrap_or(PinGraphicStyle::Line);

    let mut position = Position::new(0.0, 0.0, 0.0);
    let mut length = 0.0;
    let mut name = PinName {
        name: String::new(),
        effects: None,
    };
    let mut number = PinNumber {
        number: String::new(),
        effects: None,
    };
    let mut hide = false;

    for item in &items[2..] {
        if item.is_list_starting_with("at") {
            position = parse_position_inner(item)?;
        } else if let Some(args) = item.as_list_starting_with("length") {
            length = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
        } else if item.is_list_starting_with("name") {
            name = parse_pin_name_inner(item)?;
        } else if item.is_list_starting_with("number") {
            number = parse_pin_number_inner(item)?;
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

fn parse_pin_name_inner(sexpr: &SExpr) -> Result<PinName, String> {
    let items = sexpr
        .as_list_starting_with("name")
        .ok_or_else(|| format!("Expected name, got {:?}", sexpr))?;

    let name = items
        .first()
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut effects = None;
    for item in &items[1..] {
        if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item)?);
        }
    }

    Ok(PinName { name, effects })
}

fn parse_pin_number_inner(sexpr: &SExpr) -> Result<PinNumber, String> {
    let items = sexpr
        .as_list_starting_with("number")
        .ok_or_else(|| format!("Expected number, got {:?}", sexpr))?;

    let number = items
        .first()
        .and_then(|e| e.as_string())
        .unwrap_or("")
        .to_string();

    let mut effects = None;
    for item in &items[1..] {
        if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item)?);
        }
    }

    Ok(PinNumber { number, effects })
}

fn parse_point_inner(args: &[SExpr]) -> Result<Point, String> {
    let x = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
    let y = args.get(1).and_then(|e| e.as_number()).unwrap_or(0.0);
    Ok(Point::new(x, y))
}

fn parse_stroke_inner(sexpr: &SExpr) -> Result<Stroke, String> {
    let items = sexpr
        .as_list_starting_with("stroke")
        .ok_or_else(|| format!("Expected stroke, got {:?}", sexpr))?;

    let mut width = 0.0;
    let mut stroke_type = StrokeType::Default;
    let mut color = None;

    for item in items {
        if let Some(args) = item.as_list_starting_with("width") {
            width = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
        } else if let Some(args) = item.as_list_starting_with("type") {
            stroke_type = args
                .first()
                .and_then(|e| e.as_symbol())
                .map(StrokeType::from_str)
                .unwrap_or(StrokeType::Default);
        } else if let Some(args) = item.as_list_starting_with("color") {
            color = Some(parse_color_inner(args)?);
        }
    }

    Ok(Stroke {
        width,
        stroke_type,
        color,
    })
}

fn parse_fill_inner(sexpr: &SExpr) -> Result<Fill, String> {
    let items = sexpr
        .as_list_starting_with("fill")
        .ok_or_else(|| format!("Expected fill, got {:?}", sexpr))?;

    let mut fill_type = FillType::None;
    let mut color = None;

    for item in items {
        if let Some(args) = item.as_list_starting_with("type") {
            fill_type = args
                .first()
                .and_then(|e| e.as_symbol())
                .map(FillType::from_str)
                .unwrap_or(FillType::None);
        } else if let Some(args) = item.as_list_starting_with("color") {
            color = Some(parse_color_inner(args)?);
        }
    }

    Ok(Fill { fill_type, color })
}

fn parse_color_inner(args: &[SExpr]) -> Result<Color, String> {
    let r = args.first().and_then(|e| e.as_number()).unwrap_or(0.0) as u8;
    let g = args.get(1).and_then(|e| e.as_number()).unwrap_or(0.0) as u8;
    let b = args.get(2).and_then(|e| e.as_number()).unwrap_or(0.0) as u8;
    let a = args.get(3).and_then(|e| e.as_number()).unwrap_or(255.0) as u8;
    Ok(Color::new(r, g, b, a))
}

fn parse_junction(sexpr: &SExpr) -> Result<Junction, SchematicError> {
    let items = sexpr.as_list_starting_with("junction").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "junction".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut position = Point::default();
    let mut diameter = 0.0;
    let mut color = None;
    let mut uuid = String::new();

    for item in items {
        if let Some(args) = item.as_list_starting_with("at") {
            position = parse_point_inner(args).map_err(|e| SchematicError::Parse(e))?;
        } else if let Some(args) = item.as_list_starting_with("diameter") {
            diameter = args.first().and_then(|e| e.as_number()).unwrap_or(0.0);
        } else if let Some(args) = item.as_list_starting_with("color") {
            color = Some(parse_color_inner(args).map_err(|e| SchematicError::Parse(e))?);
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(Junction {
        position,
        diameter,
        color,
        uuid,
    })
}

fn parse_no_connect(sexpr: &SExpr) -> Result<NoConnect, SchematicError> {
    let items = sexpr.as_list_starting_with("no_connect").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "no_connect".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut position = Point::default();
    let mut uuid = String::new();

    for item in items {
        if let Some(args) = item.as_list_starting_with("at") {
            position = parse_point_inner(args).map_err(|e| SchematicError::Parse(e))?;
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(NoConnect { position, uuid })
}

fn parse_wire(sexpr: &SExpr) -> Result<Wire, SchematicError> {
    let items = sexpr.as_list_starting_with("wire").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "wire".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut points = Vec::new();
    let mut stroke = Stroke::default();
    let mut uuid = String::new();

    for item in items {
        if let Some(pts_args) = item.as_list_starting_with("pts") {
            for pt in pts_args {
                if let Some(xy_args) = pt.as_list_starting_with("xy") {
                    points.push(parse_point_inner(xy_args).map_err(|e| SchematicError::Parse(e))?);
                }
            }
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(Wire {
        points,
        stroke,
        uuid,
    })
}

fn parse_bus(sexpr: &SExpr) -> Result<Bus, SchematicError> {
    let items = sexpr.as_list_starting_with("bus").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "bus".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut points = Vec::new();
    let mut stroke = Stroke::default();
    let mut uuid = String::new();

    for item in items {
        if let Some(pts_args) = item.as_list_starting_with("pts") {
            for pt in pts_args {
                if let Some(xy_args) = pt.as_list_starting_with("xy") {
                    points.push(parse_point_inner(xy_args).map_err(|e| SchematicError::Parse(e))?);
                }
            }
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(Bus {
        points,
        stroke,
        uuid,
    })
}

fn parse_bus_entry(sexpr: &SExpr) -> Result<BusEntry, SchematicError> {
    let items = sexpr.as_list_starting_with("bus_entry").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "bus_entry".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut position = Point::default();
    let mut size = Point::default();
    let mut stroke = Stroke::default();
    let mut uuid = String::new();

    for item in items {
        if let Some(args) = item.as_list_starting_with("at") {
            position = parse_point_inner(args).map_err(|e| SchematicError::Parse(e))?;
        } else if let Some(args) = item.as_list_starting_with("size") {
            size = parse_point_inner(args).map_err(|e| SchematicError::Parse(e))?;
        } else if item.is_list_starting_with("stroke") {
            stroke = parse_stroke_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(BusEntry {
        position,
        size,
        stroke,
        uuid,
    })
}

fn parse_global_label(sexpr: &SExpr) -> Result<GlobalLabel, SchematicError> {
    let items = sexpr.as_list_starting_with("global_label").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "global_label".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let text = items
        .first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| SchematicError::MissingField("global_label text".to_string()))?
        .to_string();

    let mut shape = LabelShape::Input;
    let mut position = Position::default();
    let mut fields_autoplaced = false;
    let mut effects = None;
    let mut uuid = String::new();
    let mut properties = Vec::new();

    for item in &items[1..] {
        if let Some(args) = item.as_list_starting_with("shape") {
            shape = args
                .first()
                .and_then(|e| e.as_symbol())
                .map(LabelShape::from_str)
                .unwrap_or(LabelShape::Input);
        } else if item.is_list_starting_with("at") {
            position = parse_position_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if item.is_list_starting_with("fields_autoplaced") {
            fields_autoplaced = true;
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item).map_err(|e| SchematicError::Parse(e))?);
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        } else if item.is_list_starting_with("property") {
            properties.push(parse_property_inner(item).map_err(|e| SchematicError::Parse(e))?);
        }
    }

    Ok(GlobalLabel {
        text,
        shape,
        position,
        fields_autoplaced,
        effects,
        uuid,
        properties,
    })
}

fn parse_hierarchical_label(sexpr: &SExpr) -> Result<HierarchicalLabel, SchematicError> {
    let items = sexpr
        .as_list_starting_with("hierarchical_label")
        .ok_or_else(|| SchematicError::InvalidFormat {
            expected: "hierarchical_label".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let text = items
        .first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| SchematicError::MissingField("hierarchical_label text".to_string()))?
        .to_string();

    let mut shape = LabelShape::Input;
    let mut position = Position::default();
    let mut fields_autoplaced = false;
    let mut effects = None;
    let mut uuid = String::new();
    let mut properties = Vec::new();

    for item in &items[1..] {
        if let Some(args) = item.as_list_starting_with("shape") {
            shape = args
                .first()
                .and_then(|e| e.as_symbol())
                .map(LabelShape::from_str)
                .unwrap_or(LabelShape::Input);
        } else if item.is_list_starting_with("at") {
            position = parse_position_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if item.is_list_starting_with("fields_autoplaced") {
            fields_autoplaced = true;
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item).map_err(|e| SchematicError::Parse(e))?);
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        } else if item.is_list_starting_with("property") {
            properties.push(parse_property_inner(item).map_err(|e| SchematicError::Parse(e))?);
        }
    }

    Ok(HierarchicalLabel {
        text,
        shape,
        position,
        fields_autoplaced,
        effects,
        uuid,
        properties,
    })
}

fn parse_label(sexpr: &SExpr) -> Result<Label, SchematicError> {
    let items = sexpr.as_list_starting_with("label").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "label".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let text = items
        .first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| SchematicError::MissingField("label text".to_string()))?
        .to_string();

    let mut position = Position::default();
    let mut fields_autoplaced = false;
    let mut effects = None;
    let mut uuid = String::new();

    for item in &items[1..] {
        if item.is_list_starting_with("at") {
            position = parse_position_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if item.is_list_starting_with("fields_autoplaced") {
            fields_autoplaced = true;
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item).map_err(|e| SchematicError::Parse(e))?);
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(Label {
        text,
        position,
        fields_autoplaced,
        effects,
        uuid,
    })
}

fn parse_text_item(sexpr: &SExpr) -> Result<TextItem, SchematicError> {
    let items = sexpr.as_list_starting_with("text").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "text".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let text = items
        .first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| SchematicError::MissingField("text content".to_string()))?
        .to_string();

    let mut position = Position::default();
    let mut effects = None;
    let mut uuid = String::new();

    for item in &items[1..] {
        if item.is_list_starting_with("at") {
            position = parse_position_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if item.is_list_starting_with("effects") {
            effects = Some(parse_effects_inner(item).map_err(|e| SchematicError::Parse(e))?);
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(TextItem {
        text,
        position,
        effects,
        uuid,
    })
}

fn parse_symbol_instance(sexpr: &SExpr) -> Result<SymbolInstance, SchematicError> {
    let items = sexpr.as_list_starting_with("symbol").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "symbol".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut lib_id = String::new();
    let mut position = Position::default();
    let mut unit = 1;
    let mut exclude_from_sim = false;
    let mut in_bom = true;
    let mut on_board = true;
    let mut dnp = false;
    let mut fields_autoplaced = false;
    let mut uuid = String::new();
    let mut properties = Vec::new();
    let mut pins = Vec::new();
    let mut instances = Vec::new();
    let mut mirror = None;

    for item in items {
        if let Some(args) = item.as_list_starting_with("lib_id") {
            lib_id = args
                .first()
                .and_then(|e| e.as_string())
                .unwrap_or("")
                .to_string();
        } else if item.is_list_starting_with("at") {
            position = parse_position_inner(item).map_err(|e| SchematicError::Parse(e))?;
        } else if let Some(args) = item.as_list_starting_with("unit") {
            unit = args.first().and_then(|e| e.as_number()).unwrap_or(1.0) as u32;
        } else if let Some(args) = item.as_list_starting_with("exclude_from_sim") {
            exclude_from_sim = args.first().and_then(|e| e.as_symbol()) == Some("yes");
        } else if let Some(args) = item.as_list_starting_with("in_bom") {
            in_bom = args.first().and_then(|e| e.as_symbol()) != Some("no");
        } else if let Some(args) = item.as_list_starting_with("on_board") {
            on_board = args.first().and_then(|e| e.as_symbol()) != Some("no");
        } else if let Some(args) = item.as_list_starting_with("dnp") {
            dnp = args.first().and_then(|e| e.as_symbol()) == Some("yes");
        } else if item.is_list_starting_with("fields_autoplaced") {
            fields_autoplaced = true;
        } else if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        } else if item.is_list_starting_with("property") {
            properties.push(parse_property_inner(item).map_err(|e| SchematicError::Parse(e))?);
        } else if item.is_list_starting_with("pin") {
            pins.push(parse_pin_instance(item)?);
        } else if item.is_list_starting_with("instances") {
            instances = parse_project_instances(item)?;
        } else if let Some(args) = item.as_list_starting_with("mirror") {
            mirror = args.first().and_then(|e| e.as_symbol()).and_then(Mirror::from_str);
        }
    }

    Ok(SymbolInstance {
        lib_id,
        position,
        unit,
        exclude_from_sim,
        in_bom,
        on_board,
        dnp,
        fields_autoplaced,
        uuid,
        properties,
        pins,
        instances,
        mirror,
    })
}

fn parse_pin_instance(sexpr: &SExpr) -> Result<PinInstance, SchematicError> {
    let items = sexpr.as_list_starting_with("pin").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "pin".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let number = items
        .first()
        .and_then(|e| e.as_string())
        .ok_or_else(|| SchematicError::MissingField("pin number".to_string()))?
        .to_string();

    let mut uuid = String::new();
    for item in &items[1..] {
        if let Some(args) = item.as_list_starting_with("uuid") {
            uuid = args
                .first()
                .and_then(|e| e.as_string().or_else(|| e.as_symbol()))
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    Ok(PinInstance { number, uuid })
}

fn parse_project_instances(sexpr: &SExpr) -> Result<Vec<ProjectInstance>, SchematicError> {
    let items = sexpr.as_list_starting_with("instances").ok_or_else(|| {
        SchematicError::InvalidFormat {
            expected: "instances".to_string(),
            got: format!("{:?}", sexpr),
        }
    })?;

    let mut instances = Vec::new();

    for item in items {
        if let Some(project_args) = item.as_list_starting_with("project") {
            let project_name = project_args
                .first()
                .and_then(|e| e.as_string())
                .unwrap_or("")
                .to_string();

            let mut paths = Vec::new();
            for arg in &project_args[1..] {
                if let Some(path_args) = arg.as_list_starting_with("path") {
                    let path = path_args
                        .first()
                        .and_then(|e| e.as_string())
                        .unwrap_or("")
                        .to_string();

                    let mut reference = String::new();
                    let mut unit = 1;

                    for p in &path_args[1..] {
                        if let Some(ref_args) = p.as_list_starting_with("reference") {
                            reference = ref_args
                                .first()
                                .and_then(|e| e.as_string())
                                .unwrap_or("")
                                .to_string();
                        } else if let Some(unit_args) = p.as_list_starting_with("unit") {
                            unit = unit_args
                                .first()
                                .and_then(|e| e.as_number())
                                .unwrap_or(1.0) as u32;
                        }
                    }

                    paths.push(PathInstance {
                        path,
                        reference,
                        unit,
                    });
                }
            }

            instances.push(ProjectInstance {
                project_name,
                paths,
            });
        }
    }

    Ok(instances)
}

fn parse_sheet_instances(sexpr: &SExpr) -> Result<Vec<SheetInstance>, SchematicError> {
    let items = sexpr
        .as_list_starting_with("sheet_instances")
        .ok_or_else(|| SchematicError::InvalidFormat {
            expected: "sheet_instances".to_string(),
            got: format!("{:?}", sexpr),
        })?;

    let mut instances = Vec::new();

    for item in items {
        if let Some(path_args) = item.as_list_starting_with("path") {
            let path = path_args
                .first()
                .and_then(|e| e.as_string())
                .unwrap_or("")
                .to_string();

            let mut page = String::new();
            for arg in &path_args[1..] {
                if let Some(page_args) = arg.as_list_starting_with("page") {
                    page = page_args
                        .first()
                        .and_then(|e| e.as_string())
                        .unwrap_or("")
                        .to_string();
                }
            }

            instances.push(SheetInstance { path, page });
        }
    }

    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_schematic() {
        let input = r#"
            (kicad_sch
                (version 20231120)
                (generator "eeschema")
                (generator_version "8.0")
                (uuid "e3dd3ae4-244d-4cba-9cca-5d2abf83f29a")
                (paper "A4")
                (title_block
                    (title "Test Schematic")
                    (date "2024-01-01")
                    (rev "v1")
                )
                (lib_symbols)
            )
        "#;

        let sch = parse_schematic_str(input).unwrap();
        assert_eq!(sch.version, 20231120);
        assert_eq!(sch.generator, Some("eeschema".to_string()));
        assert_eq!(sch.paper, PaperSize::A4);
        assert!(sch.title_block.is_some());
        let tb = sch.title_block.unwrap();
        assert_eq!(tb.title, Some("Test Schematic".to_string()));
    }

    #[test]
    fn test_parse_junction_and_wire() {
        let input = r#"
            (kicad_sch
                (version 20231120)
                (generator "eeschema")
                (uuid "test-uuid")
                (paper "A4")
                (lib_symbols)
                (junction
                    (at 100 50)
                    (diameter 0)
                    (color 0 0 0 0)
                    (uuid "junction-uuid")
                )
                (wire
                    (pts
                        (xy 100 50) (xy 150 50)
                    )
                    (stroke
                        (width 0)
                        (type default)
                    )
                    (uuid "wire-uuid")
                )
            )
        "#;

        let sch = parse_schematic_str(input).unwrap();
        assert_eq!(sch.junctions.len(), 1);
        assert_eq!(sch.junctions[0].position.x, 100.0);
        assert_eq!(sch.junctions[0].position.y, 50.0);

        assert_eq!(sch.wires.len(), 1);
        assert_eq!(sch.wires[0].points.len(), 2);
        assert_eq!(sch.wires[0].points[0].x, 100.0);
        assert_eq!(sch.wires[0].points[1].x, 150.0);
    }

    #[test]
    fn test_add_wire() {
        let mut sch = Schematic::new();
        assert_eq!(sch.wires.len(), 0);

        sch.add_wire(Point::new(100.0, 50.0), Point::new(150.0, 50.0));

        assert_eq!(sch.wires.len(), 1);
        assert_eq!(sch.wires[0].points.len(), 2);
        // Should be snapped to grid (1.27mm)
        assert!((sch.wires[0].points[0].x - 100.33).abs() < 0.01);
        assert!((sch.wires[0].points[1].x - 149.86).abs() < 0.01);
    }

    #[test]
    fn test_add_wire_routed_orthogonal() {
        let mut sch = Schematic::new();

        sch.add_wire_routed(
            Point::new(100.0, 50.0),
            Point::new(150.0, 80.0),
            RoutingMode::Orthogonal,
        );

        assert_eq!(sch.wires.len(), 1);
        // Orthogonal routing creates 3 points (start, corner, end)
        assert_eq!(sch.wires[0].points.len(), 3);
    }

    #[test]
    fn test_add_wire_routed_orthogonal_vh() {
        let mut sch = Schematic::new();

        sch.add_wire_routed(
            Point::new(100.0, 50.0),
            Point::new(150.0, 80.0),
            RoutingMode::OrthogonalVH,
        );

        assert_eq!(sch.wires.len(), 1);
        assert_eq!(sch.wires[0].points.len(), 3);
        // VH mode: vertical first, then horizontal
        // Middle point should have same X as start and same Y as end
        let mid = &sch.wires[0].points[1];
        assert!((mid.x - sch.wires[0].points[0].x).abs() < 0.01);
        assert!((mid.y - sch.wires[0].points[2].y).abs() < 0.01);
    }

    #[test]
    fn test_add_label() {
        let mut sch = Schematic::new();
        assert_eq!(sch.labels.len(), 0);

        sch.add_label("TEST_NET", Position::new(100.0, 50.0, 0.0));

        assert_eq!(sch.labels.len(), 1);
        assert_eq!(sch.labels[0].text, "TEST_NET");
    }

    #[test]
    fn test_add_global_label() {
        let mut sch = Schematic::new();
        assert_eq!(sch.global_labels.len(), 0);

        sch.add_global_label("VCC", Position::new(100.0, 50.0, 0.0), LabelShape::Input);

        assert_eq!(sch.global_labels.len(), 1);
        assert_eq!(sch.global_labels[0].text, "VCC");
        assert_eq!(sch.global_labels[0].shape, LabelShape::Input);
    }

    #[test]
    fn test_delete_wire_at() {
        let mut sch = Schematic::new();
        sch.add_wire(Point::new(100.0, 50.0), Point::new(150.0, 50.0));
        assert_eq!(sch.wires.len(), 1);

        // Delete wire at a point on the wire
        let deleted = sch.delete_wire_at(Point::new(125.0, 50.0));
        assert!(deleted);
        assert_eq!(sch.wires.len(), 0);
    }

    #[test]
    fn test_delete_wire_at_not_found() {
        let mut sch = Schematic::new();
        sch.add_wire(Point::new(100.0, 50.0), Point::new(150.0, 50.0));

        // Try to delete at a point far from the wire
        let deleted = sch.delete_wire_at(Point::new(200.0, 200.0));
        assert!(!deleted);
        assert_eq!(sch.wires.len(), 1);
    }

    #[test]
    fn test_delete_label() {
        let mut sch = Schematic::new();
        sch.add_label("NET1", Position::new(100.0, 50.0, 0.0));
        sch.add_label("NET2", Position::new(150.0, 50.0, 0.0));
        assert_eq!(sch.labels.len(), 2);

        // Delete by name only
        let deleted = sch.delete_label("NET1", None);
        assert!(deleted);
        assert_eq!(sch.labels.len(), 1);
        assert_eq!(sch.labels[0].text, "NET2");
    }

    #[test]
    fn test_delete_label_by_position() {
        let mut sch = Schematic::new();
        sch.add_label("NET", Position::new(100.0, 50.0, 0.0));
        sch.add_label("NET", Position::new(150.0, 50.0, 0.0)); // Same name, different position
        assert_eq!(sch.labels.len(), 2);

        // Delete by name and position (should only delete the one at 149.86, ~50)
        let deleted = sch.delete_label("NET", Some(Point::new(149.86, 49.53)));
        assert!(deleted);
        assert_eq!(sch.labels.len(), 1);
    }

    #[test]
    fn test_delete_global_label() {
        let mut sch = Schematic::new();
        sch.add_global_label("VCC", Position::new(100.0, 50.0, 0.0), LabelShape::Input);
        assert_eq!(sch.global_labels.len(), 1);

        let deleted = sch.delete_label("VCC", None);
        assert!(deleted);
        assert_eq!(sch.global_labels.len(), 0);
    }

    #[test]
    fn test_delete_symbol_basic() {
        let mut sch = Schematic::new();

        // Manually create a minimal symbol for testing
        use crate::symbol::{Symbol, SymbolUnit, Pin, PinElectricalType, PinGraphicStyle, PinName, PinNumber};
        use crate::common::Property;

        let lib_symbol = Symbol {
            name: "Test:R".to_string(),
            pin_numbers_hide: false,
            pin_names_offset: 0.0,
            pin_names_hide: false,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            properties: vec![],
            units: vec![SymbolUnit {
                name: "R_1_1".to_string(),
                graphics: vec![],
                pins: vec![
                    Pin {
                        electrical_type: PinElectricalType::Passive,
                        graphic_style: PinGraphicStyle::Line,
                        position: Position::new(0.0, 2.54, 270.0),
                        length: 2.54,
                        name: PinName { name: "~".to_string(), effects: None },
                        number: PinNumber { number: "1".to_string(), effects: None },
                        hide: false,
                    },
                    Pin {
                        electrical_type: PinElectricalType::Passive,
                        graphic_style: PinGraphicStyle::Line,
                        position: Position::new(0.0, -2.54, 90.0),
                        length: 2.54,
                        name: PinName { name: "~".to_string(), effects: None },
                        number: PinNumber { number: "2".to_string(), effects: None },
                        hide: false,
                    },
                ],
            }],
            embedded_fonts: None,
        };

        sch.lib_symbols.push(lib_symbol);

        let symbol_instance = SymbolInstance {
            lib_id: "Test:R".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: true,
            uuid: "test-uuid".to_string(),
            properties: vec![
                Property {
                    name: "Reference".to_string(),
                    value: "R1".to_string(),
                    position: None,
                    effects: None,
                },
            ],
            pins: vec![],
            instances: vec![],
            mirror: None,
        };

        sch.symbols.push(symbol_instance);
        assert_eq!(sch.symbols.len(), 1);
        assert_eq!(sch.lib_symbols.len(), 1);

        // Delete the symbol
        let deleted = sch.delete_symbol("R1");
        assert!(deleted);
        assert_eq!(sch.symbols.len(), 0);
        // lib_symbol should also be removed since no other instance uses it
        assert_eq!(sch.lib_symbols.len(), 0);
    }

    #[test]
    fn test_delete_symbol_keeps_shared_lib_symbol() {
        let mut sch = Schematic::new();

        use crate::symbol::{Symbol, SymbolUnit};
        use crate::common::Property;

        let lib_symbol = Symbol {
            name: "Test:R".to_string(),
            pin_numbers_hide: false,
            pin_names_offset: 0.0,
            pin_names_hide: false,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            properties: vec![],
            units: vec![SymbolUnit {
                name: "R_1_1".to_string(),
                graphics: vec![],
                pins: vec![],
            }],
            embedded_fonts: None,
        };

        sch.lib_symbols.push(lib_symbol);

        // Add two instances using the same lib_symbol
        for (i, ref_name) in ["R1", "R2"].iter().enumerate() {
            let symbol_instance = SymbolInstance {
                lib_id: "Test:R".to_string(),
                position: Position::new(100.0 + (i as f64 * 20.0), 50.0, 0.0),
                unit: 1,
                exclude_from_sim: false,
                in_bom: true,
                on_board: true,
                dnp: false,
                fields_autoplaced: true,
                uuid: format!("uuid-{}", i),
                properties: vec![
                    Property {
                        name: "Reference".to_string(),
                        value: ref_name.to_string(),
                        position: None,
                        effects: None,
                    },
                ],
                pins: vec![],
                instances: vec![],
                mirror: None,
            };
            sch.symbols.push(symbol_instance);
        }

        assert_eq!(sch.symbols.len(), 2);
        assert_eq!(sch.lib_symbols.len(), 1);

        // Delete R1
        let deleted = sch.delete_symbol("R1");
        assert!(deleted);
        assert_eq!(sch.symbols.len(), 1);
        // lib_symbol should be kept since R2 still uses it
        assert_eq!(sch.lib_symbols.len(), 1);
    }

    #[test]
    fn test_delete_symbol_cleans_up_wires_and_labels() {
        let mut sch = Schematic::new();

        use crate::symbol::{Symbol, SymbolUnit, Pin, PinElectricalType, PinGraphicStyle, PinName, PinNumber};
        use crate::common::Property;

        // Create a symbol with pins at known positions
        let lib_symbol = Symbol {
            name: "Test:R".to_string(),
            pin_numbers_hide: false,
            pin_names_offset: 0.0,
            pin_names_hide: false,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            properties: vec![],
            units: vec![SymbolUnit {
                name: "R_1_1".to_string(),
                graphics: vec![],
                pins: vec![
                    Pin {
                        electrical_type: PinElectricalType::Passive,
                        graphic_style: PinGraphicStyle::Line,
                        position: Position::new(0.0, 5.08, 270.0), // Pin at top
                        length: 2.54,
                        name: PinName { name: "~".to_string(), effects: None },
                        number: PinNumber { number: "1".to_string(), effects: None },
                        hide: false,
                    },
                ],
            }],
            embedded_fonts: None,
        };

        sch.lib_symbols.push(lib_symbol);

        // Symbol at (100, 50) with pin 1 pointing up at (100, 50 - 5.08 + 2.54) = (100, 47.46)
        // Actually with 270 degree rotation: tip is at y + length in -y direction
        // Pin position is (0, 5.08) with angle 270 (pointing down from symbol center)
        // Tip = (0, 5.08) - length * (cos(270), -sin(270)) = (0, 5.08) - 2.54 * (0, 1) = (0, 2.54)
        // After translation by symbol pos (100, 50): tip at (100, 52.54)
        let symbol_instance = SymbolInstance {
            lib_id: "Test:R".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: true,
            uuid: "test-uuid".to_string(),
            properties: vec![
                Property {
                    name: "Reference".to_string(),
                    value: "R1".to_string(),
                    position: None,
                    effects: None,
                },
            ],
            pins: vec![],
            instances: vec![],
            mirror: None,
        };

        sch.symbols.push(symbol_instance);

        // Get the actual pin position
        let pin_positions = sch.get_all_pin_positions(&sch.symbols[0]);
        assert_eq!(pin_positions.len(), 1);
        let (pin_pos, _) = pin_positions[0];

        // Add a wire ending at the pin
        sch.wires.push(Wire {
            points: vec![Point::new(80.0, pin_pos.y), pin_pos],
            stroke: Stroke::default(),
            uuid: "wire-uuid".to_string(),
        });

        // Add a label at the pin position
        sch.labels.push(Label {
            text: "PIN_LABEL".to_string(),
            position: Position::new(pin_pos.x, pin_pos.y, 0.0),
            fields_autoplaced: true,
            effects: None,
            uuid: "label-uuid".to_string(),
        });

        assert_eq!(sch.wires.len(), 1);
        assert_eq!(sch.labels.len(), 1);

        // Delete the symbol
        let deleted = sch.delete_symbol("R1");
        assert!(deleted);

        // Wire and label at pin position should be deleted
        assert_eq!(sch.wires.len(), 0);
        assert_eq!(sch.labels.len(), 0);
    }

    #[test]
    fn test_delete_symbol_not_found() {
        let mut sch = Schematic::new();
        let deleted = sch.delete_symbol("R99");
        assert!(!deleted);
    }

    #[test]
    fn test_find_symbol_by_reference() {
        let mut sch = Schematic::new();

        use crate::common::Property;

        let symbol_instance = SymbolInstance {
            lib_id: "Test:R".to_string(),
            position: Position::new(100.0, 50.0, 0.0),
            unit: 1,
            exclude_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: true,
            uuid: "test-uuid".to_string(),
            properties: vec![
                Property {
                    name: "Reference".to_string(),
                    value: "R1".to_string(),
                    position: None,
                    effects: None,
                },
            ],
            pins: vec![],
            instances: vec![],
            mirror: None,
        };

        sch.symbols.push(symbol_instance);

        let found = sch.find_symbol_by_reference("R1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().lib_id, "Test:R");

        let not_found = sch.find_symbol_by_reference("R99");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_add_junction() {
        let mut sch = Schematic::new();
        assert_eq!(sch.junctions.len(), 0);

        sch.add_junction(Point::new(100.0, 50.0));

        assert_eq!(sch.junctions.len(), 1);
        // Should be snapped to grid
        assert!((sch.junctions[0].position.x - 100.33).abs() < 0.01);
    }
}
