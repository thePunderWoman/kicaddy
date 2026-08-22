//! Serialization support for KiCAD schematics

use std::io::Write;
use std::path::Path;

use crate::common::{Effects, Fill, Font, Justify, Point, Position, Property, Stroke};
use crate::parser::sexpr::{SExpr, ToSExpr};
use crate::symbol::Symbol;

use super::types::*;

impl Schematic {
    /// Write the schematic to a file
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let content = self.to_sexpr().to_kicad_string();
        let mut file = std::fs::File::create(path)?;
        file.write_all(content.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }

    /// Convert to KiCAD-formatted string
    pub fn to_kicad_string(&self) -> String {
        self.to_sexpr().to_kicad_string()
    }
}

impl ToSExpr for Schematic {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("kicad_sch")];

        items.push(SExpr::list(vec![
            SExpr::symbol("version"),
            SExpr::number(self.version as f64),
        ]));

        if let Some(ref generator) = self.generator {
            items.push(SExpr::list(vec![
                SExpr::symbol("generator"),
                SExpr::string(generator),
            ]));
        }

        if let Some(ref generator_version) = self.generator_version {
            items.push(SExpr::list(vec![
                SExpr::symbol("generator_version"),
                SExpr::string(generator_version),
            ]));
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("paper"),
            SExpr::string(self.paper.as_str()),
        ]));

        if let Some(ref tb) = self.title_block {
            items.push(tb.to_sexpr());
        }

        // lib_symbols
        let mut lib_items = vec![SExpr::symbol("lib_symbols")];
        for sym in &self.lib_symbols {
            lib_items.push(sym.to_sexpr());
        }
        items.push(SExpr::list(lib_items));

        // junctions
        for junction in &self.junctions {
            items.push(junction.to_sexpr());
        }

        // no_connects
        for nc in &self.no_connects {
            items.push(nc.to_sexpr());
        }

        // wires
        for wire in &self.wires {
            items.push(wire.to_sexpr());
        }

        // buses
        for bus in &self.buses {
            items.push(bus.to_sexpr());
        }

        // bus_entries
        for entry in &self.bus_entries {
            items.push(entry.to_sexpr());
        }

        // global_labels
        for label in &self.global_labels {
            items.push(label.to_sexpr());
        }

        // hierarchical_labels
        for label in &self.hierarchical_labels {
            items.push(label.to_sexpr());
        }

        // labels
        for label in &self.labels {
            items.push(label.to_sexpr());
        }

        // text items
        for text in &self.text_items {
            items.push(text.to_sexpr());
        }

        // symbol instances
        for sym in &self.symbols {
            items.push(sym.to_sexpr());
        }

        // sheets
        for sheet in &self.sheets {
            items.push(sheet.to_sexpr());
        }

        // sheet_instances
        if !self.sheet_instances.is_empty() {
            let mut sheet_items = vec![SExpr::symbol("sheet_instances")];
            for inst in &self.sheet_instances {
                sheet_items.push(inst.to_sexpr());
            }
            items.push(SExpr::list(sheet_items));
        }

        // embedded_fonts
        items.push(SExpr::list(vec![
            SExpr::symbol("embedded_fonts"),
            SExpr::symbol(if self.embedded_fonts { "yes" } else { "no" }),
        ]));

        SExpr::list(items)
    }
}

impl ToSExpr for TitleBlock {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("title_block")];

        if let Some(ref title) = self.title {
            items.push(SExpr::list(vec![
                SExpr::symbol("title"),
                SExpr::string(title),
            ]));
        }

        if let Some(ref date) = self.date {
            items.push(SExpr::list(vec![
                SExpr::symbol("date"),
                SExpr::string(date),
            ]));
        }

        if let Some(ref rev) = self.rev {
            items.push(SExpr::list(vec![
                SExpr::symbol("rev"),
                SExpr::string(rev),
            ]));
        }

        if let Some(ref company) = self.company {
            items.push(SExpr::list(vec![
                SExpr::symbol("company"),
                SExpr::string(company),
            ]));
        }

        for (idx, text) in &self.comments {
            items.push(SExpr::list(vec![
                SExpr::symbol("comment"),
                SExpr::number(*idx as f64),
                SExpr::string(text),
            ]));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Junction {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("junction")];

        items.push(SExpr::list(vec![
            SExpr::symbol("at"),
            SExpr::number(self.position.x),
            SExpr::number(self.position.y),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("diameter"),
            SExpr::number(self.diameter),
        ]));

        if let Some(ref color) = self.color {
            items.push(SExpr::list(vec![
                SExpr::symbol("color"),
                SExpr::number(color.r as f64),
                SExpr::number(color.g as f64),
                SExpr::number(color.b as f64),
                SExpr::number(color.a as f64),
            ]));
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        SExpr::list(items)
    }
}

impl ToSExpr for NoConnect {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("no_connect"),
            SExpr::list(vec![
                SExpr::symbol("at"),
                SExpr::number(self.position.x),
                SExpr::number(self.position.y),
            ]),
            SExpr::list(vec![SExpr::symbol("uuid"), SExpr::string(&self.uuid)]),
        ])
    }
}

impl ToSExpr for Wire {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("wire")];

        let mut pts = vec![SExpr::symbol("pts")];
        for pt in &self.points {
            pts.push(SExpr::list(vec![
                SExpr::symbol("xy"),
                SExpr::number(pt.x),
                SExpr::number(pt.y),
            ]));
        }
        items.push(SExpr::list(pts));

        items.push(self.stroke.to_sexpr());

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        SExpr::list(items)
    }
}

impl ToSExpr for Bus {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("bus")];

        let mut pts = vec![SExpr::symbol("pts")];
        for pt in &self.points {
            pts.push(SExpr::list(vec![
                SExpr::symbol("xy"),
                SExpr::number(pt.x),
                SExpr::number(pt.y),
            ]));
        }
        items.push(SExpr::list(pts));

        items.push(self.stroke.to_sexpr());

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        SExpr::list(items)
    }
}

impl ToSExpr for BusEntry {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("bus_entry")];

        items.push(SExpr::list(vec![
            SExpr::symbol("at"),
            SExpr::number(self.position.x),
            SExpr::number(self.position.y),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("size"),
            SExpr::number(self.size.x),
            SExpr::number(self.size.y),
        ]));

        items.push(self.stroke.to_sexpr());

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        SExpr::list(items)
    }
}

impl ToSExpr for TextItem {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("text"), SExpr::string(&self.text)];

        items.push(self.position.to_sexpr());

        if let Some(ref effects) = self.effects {
            items.push(effects.to_sexpr());
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        SExpr::list(items)
    }
}

impl ToSExpr for GlobalLabel {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("global_label"), SExpr::string(&self.text)];

        items.push(SExpr::list(vec![
            SExpr::symbol("shape"),
            SExpr::symbol(self.shape.as_str()),
        ]));

        items.push(self.position.to_sexpr());

        if self.fields_autoplaced {
            items.push(SExpr::list(vec![
                SExpr::symbol("fields_autoplaced"),
                SExpr::symbol("yes"),
            ]));
        }

        if let Some(ref effects) = self.effects {
            items.push(effects.to_sexpr());
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        for prop in &self.properties {
            items.push(prop.to_sexpr());
        }

        SExpr::list(items)
    }
}

impl ToSExpr for HierarchicalLabel {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![
            SExpr::symbol("hierarchical_label"),
            SExpr::string(&self.text),
        ];

        items.push(SExpr::list(vec![
            SExpr::symbol("shape"),
            SExpr::symbol(self.shape.as_str()),
        ]));

        items.push(self.position.to_sexpr());

        if self.fields_autoplaced {
            items.push(SExpr::list(vec![
                SExpr::symbol("fields_autoplaced"),
                SExpr::symbol("yes"),
            ]));
        }

        if let Some(ref effects) = self.effects {
            items.push(effects.to_sexpr());
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        for prop in &self.properties {
            items.push(prop.to_sexpr());
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Label {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("label"), SExpr::string(&self.text)];

        items.push(self.position.to_sexpr());

        if self.fields_autoplaced {
            items.push(SExpr::list(vec![
                SExpr::symbol("fields_autoplaced"),
                SExpr::symbol("yes"),
            ]));
        }

        if let Some(ref effects) = self.effects {
            items.push(effects.to_sexpr());
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        SExpr::list(items)
    }
}

impl ToSExpr for SymbolInstance {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("symbol")];

        items.push(SExpr::list(vec![
            SExpr::symbol("lib_id"),
            SExpr::string(&self.lib_id),
        ]));

        items.push(self.position.to_sexpr());

        if let Some(mirror) = self.mirror {
            items.push(SExpr::list(vec![
                SExpr::symbol("mirror"),
                SExpr::symbol(mirror.as_str()),
            ]));
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("unit"),
            SExpr::number(self.unit as f64),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("exclude_from_sim"),
            SExpr::symbol(if self.exclude_from_sim { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("in_bom"),
            SExpr::symbol(if self.in_bom { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("on_board"),
            SExpr::symbol(if self.on_board { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("dnp"),
            SExpr::symbol(if self.dnp { "yes" } else { "no" }),
        ]));

        if self.fields_autoplaced {
            items.push(SExpr::list(vec![
                SExpr::symbol("fields_autoplaced"),
                SExpr::symbol("yes"),
            ]));
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        for prop in &self.properties {
            items.push(prop.to_sexpr());
        }

        for pin in &self.pins {
            items.push(pin.to_sexpr());
        }

        if !self.instances.is_empty() {
            let mut inst_items = vec![SExpr::symbol("instances")];
            for inst in &self.instances {
                inst_items.push(inst.to_sexpr());
            }
            items.push(SExpr::list(inst_items));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for PinInstance {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("pin"),
            SExpr::string(&self.number),
            SExpr::list(vec![SExpr::symbol("uuid"), SExpr::string(&self.uuid)]),
        ])
    }
}

impl ToSExpr for ProjectInstance {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("project"), SExpr::string(&self.project_name)];

        for path in &self.paths {
            items.push(path.to_sexpr());
        }

        SExpr::list(items)
    }
}

impl ToSExpr for PathInstance {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("path"),
            SExpr::string(&self.path),
            SExpr::list(vec![
                SExpr::symbol("reference"),
                SExpr::string(&self.reference),
            ]),
            SExpr::list(vec![
                SExpr::symbol("unit"),
                SExpr::number(self.unit as f64),
            ]),
        ])
    }
}

impl ToSExpr for SheetInstance {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("path"),
            SExpr::string(&self.path),
            SExpr::list(vec![SExpr::symbol("page"), SExpr::string(&self.page)]),
        ])
    }
}

impl ToSExpr for SheetProjectInstance {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("project"), SExpr::string(&self.project_name)];

        for path in &self.paths {
            items.push(path.to_sexpr());
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Sheet {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("sheet")];

        // Position (at x y) - no angle for sheets
        items.push(SExpr::list(vec![
            SExpr::symbol("at"),
            SExpr::number(self.position.x),
            SExpr::number(self.position.y),
        ]));

        // Size
        items.push(SExpr::list(vec![
            SExpr::symbol("size"),
            SExpr::number(self.size.0),
            SExpr::number(self.size.1),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("exclude_from_sim"),
            SExpr::symbol(if self.exclude_from_sim { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("in_bom"),
            SExpr::symbol(if self.in_bom { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("on_board"),
            SExpr::symbol(if self.on_board { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("dnp"),
            SExpr::symbol(if self.dnp { "yes" } else { "no" }),
        ]));

        if self.fields_autoplaced {
            items.push(SExpr::list(vec![
                SExpr::symbol("fields_autoplaced"),
                SExpr::symbol("yes"),
            ]));
        }

        // Stroke (default)
        items.push(SExpr::list(vec![
            SExpr::symbol("stroke"),
            SExpr::list(vec![SExpr::symbol("width"), SExpr::number(0.1524)]),
            SExpr::list(vec![SExpr::symbol("type"), SExpr::symbol("solid")]),
        ]));

        // Fill
        items.push(SExpr::list(vec![
            SExpr::symbol("fill"),
            SExpr::list(vec![
                SExpr::symbol("color"),
                SExpr::number(0.0),
                SExpr::number(0.0),
                SExpr::number(0.0),
                SExpr::number(0.0),
            ]),
        ]));

        // UUID
        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        // Sheetname property
        items.push(SExpr::list(vec![
            SExpr::symbol("property"),
            SExpr::string("Sheetname"),
            SExpr::string(&self.sheet_name),
            SExpr::list(vec![
                SExpr::symbol("at"),
                SExpr::number(self.position.x),
                SExpr::number(self.position.y - 0.7116),
                SExpr::number(0.0),
            ]),
            SExpr::list(vec![
                SExpr::symbol("effects"),
                SExpr::list(vec![
                    SExpr::symbol("font"),
                    SExpr::list(vec![
                        SExpr::symbol("size"),
                        SExpr::number(1.27),
                        SExpr::number(1.27),
                    ]),
                ]),
                SExpr::list(vec![
                    SExpr::symbol("justify"),
                    SExpr::symbol("left"),
                    SExpr::symbol("bottom"),
                ]),
            ]),
        ]));

        // Sheetfile property
        items.push(SExpr::list(vec![
            SExpr::symbol("property"),
            SExpr::string("Sheetfile"),
            SExpr::string(&self.sheet_file),
            SExpr::list(vec![
                SExpr::symbol("at"),
                SExpr::number(self.position.x),
                SExpr::number(self.position.y + self.size.1 + 0.5846),
                SExpr::number(0.0),
            ]),
            SExpr::list(vec![
                SExpr::symbol("effects"),
                SExpr::list(vec![
                    SExpr::symbol("font"),
                    SExpr::list(vec![
                        SExpr::symbol("size"),
                        SExpr::number(1.27),
                        SExpr::number(1.27),
                    ]),
                ]),
                SExpr::list(vec![
                    SExpr::symbol("justify"),
                    SExpr::symbol("left"),
                    SExpr::symbol("top"),
                ]),
            ]),
        ]));

        // Pins
        for pin in &self.pins {
            items.push(pin.to_sexpr());
        }

        // Instances (page-number bookkeeping) — omitted when empty so a standalone Sheet built
        // outside compile() doesn't emit an empty `(instances)` block.
        if !self.instances.is_empty() {
            let mut instance_items = vec![SExpr::symbol("instances")];
            for instance in &self.instances {
                instance_items.push(instance.to_sexpr());
            }
            items.push(SExpr::list(instance_items));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for SheetPin {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![
            SExpr::symbol("pin"),
            SExpr::string(&self.name),
            SExpr::symbol(self.shape.as_str()),
        ];

        items.push(self.position.to_sexpr());

        items.push(SExpr::list(vec![
            SExpr::symbol("uuid"),
            SExpr::string(&self.uuid),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("effects"),
            SExpr::list(vec![
                SExpr::symbol("font"),
                SExpr::list(vec![
                    SExpr::symbol("size"),
                    SExpr::number(1.27),
                    SExpr::number(1.27),
                ]),
            ]),
            SExpr::list(vec![
                SExpr::symbol("justify"),
                SExpr::symbol("right"),
            ]),
        ]));

        SExpr::list(items)
    }
}

// Common types

impl ToSExpr for Position {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("at"),
            SExpr::number(self.x),
            SExpr::number(self.y),
            SExpr::number(self.angle),
        ])
    }
}

impl ToSExpr for Point {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("xy"),
            SExpr::number(self.x),
            SExpr::number(self.y),
        ])
    }
}

impl ToSExpr for Stroke {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("stroke")];

        items.push(SExpr::list(vec![
            SExpr::symbol("width"),
            SExpr::number(self.width),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("type"),
            SExpr::symbol(self.stroke_type.as_str()),
        ]));

        if let Some(ref color) = self.color {
            items.push(SExpr::list(vec![
                SExpr::symbol("color"),
                SExpr::number(color.r as f64),
                SExpr::number(color.g as f64),
                SExpr::number(color.b as f64),
                SExpr::number(color.a as f64),
            ]));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Effects {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("effects")];

        if let Some(ref font) = self.font {
            items.push(font.to_sexpr());
        }

        if let Some(ref justify) = self.justify {
            items.push(justify.to_sexpr());
        }

        if self.hide {
            items.push(SExpr::list(vec![
                SExpr::symbol("hide"),
                SExpr::symbol("yes"),
            ]));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Font {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("font")];

        if let Some((w, h)) = self.size {
            items.push(SExpr::list(vec![
                SExpr::symbol("size"),
                SExpr::number(w),
                SExpr::number(h),
            ]));
        }

        if let Some(thickness) = self.thickness {
            items.push(SExpr::list(vec![
                SExpr::symbol("thickness"),
                SExpr::number(thickness),
            ]));
        }

        if self.bold {
            items.push(SExpr::symbol("bold"));
        }

        if self.italic {
            items.push(SExpr::symbol("italic"));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Justify {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("justify")];

        match self.horizontal {
            crate::common::HorizontalJustify::Left => items.push(SExpr::symbol("left")),
            crate::common::HorizontalJustify::Right => items.push(SExpr::symbol("right")),
            crate::common::HorizontalJustify::Center => {}
        }

        match self.vertical {
            crate::common::VerticalJustify::Top => items.push(SExpr::symbol("top")),
            crate::common::VerticalJustify::Bottom => items.push(SExpr::symbol("bottom")),
            crate::common::VerticalJustify::Center => {}
        }

        if self.mirror {
            items.push(SExpr::symbol("mirror"));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Property {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![
            SExpr::symbol("property"),
            SExpr::string(&self.name),
            SExpr::string(&self.value),
        ];

        if let Some(ref pos) = self.position {
            items.push(pos.to_sexpr());
        }

        if let Some(ref effects) = self.effects {
            items.push(effects.to_sexpr());
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Symbol {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("symbol"), SExpr::string(&self.name)];

        // pin_numbers
        if self.pin_numbers_hide {
            items.push(SExpr::list(vec![
                SExpr::symbol("pin_numbers"),
                SExpr::list(vec![SExpr::symbol("hide"), SExpr::symbol("yes")]),
            ]));
        }

        // pin_names
        let mut pin_names = vec![SExpr::symbol("pin_names")];
        if self.pin_names_offset != 0.0 {
            pin_names.push(SExpr::list(vec![
                SExpr::symbol("offset"),
                SExpr::number(self.pin_names_offset),
            ]));
        }
        if self.pin_names_hide {
            pin_names.push(SExpr::symbol("hide"));
        }
        if pin_names.len() > 1 {
            items.push(SExpr::list(pin_names));
        }

        items.push(SExpr::list(vec![
            SExpr::symbol("exclude_from_sim"),
            SExpr::symbol(if self.exclude_from_sim { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("in_bom"),
            SExpr::symbol(if self.in_bom { "yes" } else { "no" }),
        ]));

        items.push(SExpr::list(vec![
            SExpr::symbol("on_board"),
            SExpr::symbol(if self.on_board { "yes" } else { "no" }),
        ]));

        for prop in &self.properties {
            items.push(prop.to_sexpr());
        }

        for unit in &self.units {
            items.push(unit.to_sexpr());
        }

        if let Some(embedded) = self.embedded_fonts {
            items.push(SExpr::list(vec![
                SExpr::symbol("embedded_fonts"),
                SExpr::symbol(if embedded { "yes" } else { "no" }),
            ]));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for crate::symbol::SymbolUnit {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("symbol"), SExpr::string(&self.name)];

        for graphic in &self.graphics {
            items.push(graphic.to_sexpr());
        }

        for pin in &self.pins {
            items.push(pin.to_sexpr());
        }

        SExpr::list(items)
    }
}

impl ToSExpr for crate::symbol::GraphicItem {
    fn to_sexpr(&self) -> SExpr {
        match self {
            crate::symbol::GraphicItem::Rectangle(r) => r.to_sexpr(),
            crate::symbol::GraphicItem::Polyline(p) => p.to_sexpr(),
            crate::symbol::GraphicItem::Circle(c) => c.to_sexpr(),
            crate::symbol::GraphicItem::Arc(a) => a.to_sexpr(),
            crate::symbol::GraphicItem::Text(t) => t.to_sexpr(),
        }
    }
}

impl ToSExpr for crate::symbol::graphics::Rectangle {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("rectangle"),
            SExpr::list(vec![
                SExpr::symbol("start"),
                SExpr::number(self.start.x),
                SExpr::number(self.start.y),
            ]),
            SExpr::list(vec![
                SExpr::symbol("end"),
                SExpr::number(self.end.x),
                SExpr::number(self.end.y),
            ]),
            self.stroke.to_sexpr(),
            self.fill.to_sexpr(),
        ])
    }
}

impl ToSExpr for crate::symbol::graphics::Polyline {
    fn to_sexpr(&self) -> SExpr {
        let mut pts = vec![SExpr::symbol("pts")];
        for pt in &self.points {
            pts.push(pt.to_sexpr());
        }

        SExpr::list(vec![
            SExpr::symbol("polyline"),
            SExpr::list(pts),
            self.stroke.to_sexpr(),
            self.fill.to_sexpr(),
        ])
    }
}

impl ToSExpr for crate::symbol::graphics::Circle {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("circle"),
            SExpr::list(vec![
                SExpr::symbol("center"),
                SExpr::number(self.center.x),
                SExpr::number(self.center.y),
            ]),
            SExpr::list(vec![SExpr::symbol("radius"), SExpr::number(self.radius)]),
            self.stroke.to_sexpr(),
            self.fill.to_sexpr(),
        ])
    }
}

impl ToSExpr for crate::symbol::graphics::Arc {
    fn to_sexpr(&self) -> SExpr {
        SExpr::list(vec![
            SExpr::symbol("arc"),
            SExpr::list(vec![
                SExpr::symbol("start"),
                SExpr::number(self.start.x),
                SExpr::number(self.start.y),
            ]),
            SExpr::list(vec![
                SExpr::symbol("mid"),
                SExpr::number(self.mid.x),
                SExpr::number(self.mid.y),
            ]),
            SExpr::list(vec![
                SExpr::symbol("end"),
                SExpr::number(self.end.x),
                SExpr::number(self.end.y),
            ]),
            self.stroke.to_sexpr(),
            self.fill.to_sexpr(),
        ])
    }
}

impl ToSExpr for crate::symbol::graphics::Text {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("text"), SExpr::string(&self.text)];

        items.push(self.position.to_sexpr());

        if let Some(ref effects) = self.effects {
            items.push(effects.to_sexpr());
        }

        SExpr::list(items)
    }
}

impl ToSExpr for Fill {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![SExpr::symbol("fill")];

        items.push(SExpr::list(vec![
            SExpr::symbol("type"),
            SExpr::symbol(self.fill_type.as_str()),
        ]));

        if let Some(ref color) = self.color {
            items.push(SExpr::list(vec![
                SExpr::symbol("color"),
                SExpr::number(color.r as f64),
                SExpr::number(color.g as f64),
                SExpr::number(color.b as f64),
                SExpr::number(color.a as f64),
            ]));
        }

        SExpr::list(items)
    }
}

impl ToSExpr for crate::symbol::Pin {
    fn to_sexpr(&self) -> SExpr {
        let mut items = vec![
            SExpr::symbol("pin"),
            SExpr::symbol(self.electrical_type.as_str()),
            SExpr::symbol(self.graphic_style.as_str()),
        ];

        items.push(self.position.to_sexpr());

        items.push(SExpr::list(vec![
            SExpr::symbol("length"),
            SExpr::number(self.length),
        ]));

        // hide must come before name (KiCAD format requirement)
        if self.hide {
            items.push(SExpr::list(vec![
                SExpr::symbol("hide"),
                SExpr::symbol("yes"),
            ]));
        }

        // name
        let mut name_items = vec![SExpr::symbol("name"), SExpr::string(&self.name.name)];
        if let Some(ref effects) = self.name.effects {
            name_items.push(effects.to_sexpr());
        }
        items.push(SExpr::list(name_items));

        // number
        let mut num_items = vec![SExpr::symbol("number"), SExpr::string(&self.number.number)];
        if let Some(ref effects) = self.number.effects {
            num_items.push(effects.to_sexpr());
        }
        items.push(SExpr::list(num_items));

        SExpr::list(items)
    }
}
