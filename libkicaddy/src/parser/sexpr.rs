//! Generic S-expression parser for KiCAD files
//!
//! KiCAD uses a Lisp-like S-expression format. This module provides
//! low-level parsing primitives that can be composed for specific file types.

use nom::{
    Parser,
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_while, take_while1},
    character::complete::{char, multispace0, none_of},
    combinator::{cut, map, opt, recognize, value},
    error::context,
    multi::many0,
    sequence::{delimited, preceded},
};
use nom_language::error::{VerboseError, convert_error};

/// Result type using VerboseError for better error messages
pub type ParseResult<'a, T> = Result<(&'a str, T), nom::Err<VerboseError<&'a str>>>;

/// Format a parse error with context for user-friendly display
pub fn format_error(input: &str, e: &nom::Err<VerboseError<&str>>) -> String {
    match e {
        nom::Err::Incomplete(_) => "Incomplete input".to_string(),
        nom::Err::Error(ve) | nom::Err::Failure(ve) => {
            convert_error(input, ve.clone())
        }
    }
}

/// An S-expression value
#[derive(Debug, Clone, PartialEq)]
pub enum SExpr {
    /// A symbol/identifier like `kicad_symbol_lib` or `pin`
    Symbol(String),
    /// A quoted string like `"Resistor"`
    String(String),
    /// A numeric value
    Number(f64),
    /// A list of S-expressions like `(property "Value" "R")`
    List(Vec<SExpr>),
}

impl SExpr {
    /// Create a symbol S-expression
    pub fn symbol(s: impl Into<String>) -> Self {
        SExpr::Symbol(s.into())
    }

    /// Create a string S-expression
    pub fn string(s: impl Into<String>) -> Self {
        SExpr::String(s.into())
    }

    /// Create a number S-expression
    pub fn number(n: f64) -> Self {
        SExpr::Number(n)
    }

    /// Create a list S-expression
    pub fn list(items: Vec<SExpr>) -> Self {
        SExpr::List(items)
    }

    /// Get the symbol name if this is a Symbol
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            SExpr::Symbol(s) => Some(s),
            _ => None,
        }
    }

    /// Get the string value if this is a String
    pub fn as_string(&self) -> Option<&str> {
        match self {
            SExpr::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get the number if this is a Number
    pub fn as_number(&self) -> Option<f64> {
        match self {
            SExpr::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Get the list contents if this is a List
    pub fn as_list(&self) -> Option<&[SExpr]> {
        match self {
            SExpr::List(l) => Some(l),
            _ => None,
        }
    }

    /// Check if this is a list starting with the given symbol
    pub fn is_list_starting_with(&self, name: &str) -> bool {
        match self {
            SExpr::List(items) => items.first().map(|e| e.as_symbol()) == Some(Some(name)),
            _ => false,
        }
    }

    /// Get list contents if it starts with the given symbol
    pub fn as_list_starting_with(&self, name: &str) -> Option<&[SExpr]> {
        match self {
            SExpr::List(items) if items.first().and_then(|e| e.as_symbol()) == Some(name) => {
                Some(&items[1..])
            }
            _ => None,
        }
    }

    /// Convert to KiCAD-formatted string
    pub fn to_kicad_string(&self) -> String {
        let mut output = String::new();
        self.write_kicad_string(&mut output, 0);
        output
    }

    /// Write KiCAD-formatted string with indentation
    fn write_kicad_string(&self, output: &mut String, indent: usize) {
        match self {
            SExpr::Symbol(s) => output.push_str(s),
            SExpr::String(s) => {
                output.push('"');
                for c in s.chars() {
                    match c {
                        '"' => output.push_str("\\\""),
                        '\\' => output.push_str("\\\\"),
                        '\n' => output.push_str("\\n"),
                        '\r' => output.push_str("\\r"),
                        '\t' => output.push_str("\\t"),
                        _ => output.push(c),
                    }
                }
                output.push('"');
            }
            SExpr::Number(n) => {
                // Format numbers to match KiCAD's style (avoid unnecessary decimal places)
                if n.fract() == 0.0 && n.abs() < 1e10 {
                    output.push_str(&format!("{}", *n as i64));
                } else {
                    // Use a reasonable precision for floats
                    let formatted = format!("{:.6}", n);
                    // Trim trailing zeros after decimal point
                    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
                    output.push_str(trimmed);
                }
            }
            SExpr::List(items) => {
                if items.is_empty() {
                    output.push_str("()");
                    return;
                }

                // Check if this is a top-level structure that needs multiline formatting
                let is_top_level = indent == 0;
                let first_sym = items.first().and_then(|e| e.as_symbol());
                let needs_multiline = is_top_level
                    || matches!(
                        first_sym,
                        Some(
                            "kicad_sch"
                                | "lib_symbols"
                                | "symbol"
                                | "property"
                                | "junction"
                                | "wire"
                                | "bus"
                                | "global_label"
                                | "hierarchical_label"
                                | "label"
                                | "text"
                                | "title_block"
                                | "sheet_instances"
                                | "instances"
                                | "project"
                                | "path"
                        )
                    );

                output.push('(');

                if needs_multiline && items.len() > 1 {
                    // First item on same line
                    items[0].write_kicad_string(output, indent + 1);

                    // Check if second item is a string (like symbol names)
                    if let Some(SExpr::String(_)) = items.get(1) {
                        output.push(' ');
                        items[1].write_kicad_string(output, indent + 1);
                        // Rest on new lines
                        for item in &items[2..] {
                            output.push('\n');
                            for _ in 0..(indent + 1) {
                                output.push('\t');
                            }
                            item.write_kicad_string(output, indent + 1);
                        }
                    } else {
                        // Rest on new lines
                        for item in &items[1..] {
                            output.push('\n');
                            for _ in 0..(indent + 1) {
                                output.push('\t');
                            }
                            item.write_kicad_string(output, indent + 1);
                        }
                    }
                    output.push('\n');
                    for _ in 0..indent {
                        output.push('\t');
                    }
                } else {
                    // Single line format
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            output.push(' ');
                        }
                        item.write_kicad_string(output, indent + 1);
                    }
                }

                output.push(')');
            }
        }
    }
}

/// Trait for types that can be converted to S-expressions
pub trait ToSExpr {
    /// Convert this value to an S-expression
    fn to_sexpr(&self) -> SExpr;
}

/// Parser for KiCAD S-expressions
pub struct SExprParser;

impl SExprParser {
    /// Parse a complete S-expression from input
    pub fn parse(input: &str) -> ParseResult<'_, SExpr> {
        preceded(multispace0, Self::sexpr).parse(input)
    }

    /// Parse an S-expression
    fn sexpr(input: &str) -> ParseResult<'_, SExpr> {
        context(
            "S-expression",
            alt((Self::list, Self::string, Self::number, Self::symbol))
        ).parse(input)
    }

    /// Parse a list: (...)
    fn list(input: &str) -> ParseResult<'_, SExpr> {
        context(
            "list",
            map(
                delimited(
                    char('('),
                    cut(many0(preceded(multispace0, Self::sexpr))),
                    cut(context("closing ')'", preceded(multispace0, char(')')))),
                ),
                SExpr::List,
            )
        ).parse(input)
    }

    /// Parse a quoted string: "..."
    fn string(input: &str) -> ParseResult<'_, SExpr> {
        context(
            "string",
            map(
                delimited(
                    char('"'),
                    cut(map(
                        opt(escaped_transform(
                            none_of("\\\""),
                            '\\',
                            alt((
                                value("\\", tag("\\")),
                                value("\"", tag("\"")),
                                value("\n", tag("n")),
                                value("\r", tag("r")),
                                value("\t", tag("t")),
                            )),
                        )),
                        |s| s.unwrap_or_default(),
                    )),
                    cut(context("closing '\"'", char('"'))),
                ),
                SExpr::String,
            )
        ).parse(input)
    }

    /// Parse a number (integer or float)
    fn number(input: &str) -> ParseResult<'_, SExpr> {
        context(
            "number",
            map(
                recognize(|input| {
                    let (input, _) = opt(char('-')).parse(input)?;
                    let (input, _) = take_while1(|c: char| c.is_ascii_digit()).parse(input)?;
                    let (input, _) = opt(|input| {
                        let (input, _) = char('.').parse(input)?;
                        take_while(|c: char| c.is_ascii_digit()).parse(input)
                    }).parse(input)?;
                    let (input, _) = opt(|input| {
                        let (input, _) = alt((char('e'), char('E'))).parse(input)?;
                        let (input, _) = opt(alt((char('+'), char('-')))).parse(input)?;
                        take_while1(|c: char| c.is_ascii_digit()).parse(input)
                    }).parse(input)?;
                    Ok((input, ()))
                }),
                |s: &str| SExpr::Number(s.parse().unwrap()),
            )
        ).parse(input)
    }

    /// Parse a symbol/identifier
    fn symbol(input: &str) -> ParseResult<'_, SExpr> {
        context(
            "symbol",
            map(
                take_while1(|c: char| {
                    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '~' | '+' | '*')
                }),
                |s: &str| SExpr::Symbol(s.to_string()),
            )
        ).parse(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_symbol() {
        let (rest, expr) = SExprParser::parse("kicad_symbol_lib").unwrap();
        assert_eq!(rest, "");
        assert_eq!(expr, SExpr::Symbol("kicad_symbol_lib".to_string()));
    }

    #[test]
    fn test_parse_string() {
        let (rest, expr) = SExprParser::parse(r#""Resistor""#).unwrap();
        assert_eq!(rest, "");
        assert_eq!(expr, SExpr::String("Resistor".to_string()));
    }

    #[test]
    fn test_parse_string_with_escape() {
        let (rest, expr) = SExprParser::parse(r#""Hello \"World\"""#).unwrap();
        assert_eq!(rest, "");
        assert_eq!(expr, SExpr::String("Hello \"World\"".to_string()));
    }

    #[test]
    fn test_parse_number() {
        let (rest, expr) = SExprParser::parse("42").unwrap();
        assert_eq!(rest, "");
        assert_eq!(expr, SExpr::Number(42.0));

        let (rest, expr) = SExprParser::parse("-3.14").unwrap();
        assert_eq!(rest, "");
        assert_eq!(expr, SExpr::Number(-3.14));

        let (rest, expr) = SExprParser::parse("1.5e-3").unwrap();
        assert_eq!(rest, "");
        assert_eq!(expr, SExpr::Number(0.0015));
    }

    #[test]
    fn test_parse_simple_list() {
        let (rest, expr) = SExprParser::parse("(version 20241209)").unwrap();
        assert_eq!(rest, "");
        assert!(matches!(expr, SExpr::List(_)));
        if let SExpr::List(items) = expr {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], SExpr::Symbol("version".to_string()));
            assert_eq!(items[1], SExpr::Number(20241209.0));
        }
    }

    #[test]
    fn test_parse_nested_list() {
        let input = r#"(property "Reference" "R" (at 0 0 90))"#;
        let (rest, expr) = SExprParser::parse(input).unwrap();
        assert_eq!(rest, "");
        if let SExpr::List(items) = &expr {
            assert_eq!(items.len(), 4);
            assert!(items[3].is_list_starting_with("at"));
        }
    }

    #[test]
    fn test_as_list_starting_with() {
        let input = "(pin passive line)";
        let (_, expr) = SExprParser::parse(input).unwrap();
        let contents = expr.as_list_starting_with("pin").unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].as_symbol(), Some("passive"));
    }
}
