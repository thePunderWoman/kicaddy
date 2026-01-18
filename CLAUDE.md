# kicaddy

Rust library and CLI for manipulating KiCAD files, with a focus on schematic editing.

## Project Structure

- `kicaddy/` - CLI binary
- `libkicaddy/` - Core library
  - `common/` - Shared types (Position, Point, Property, Effects, Stroke, Color)
  - `config.rs` - KiCAD installation detection
  - `parser/` - S-expression parser and serializer
  - `schematic/` - Schematic file parsing, types, and serialization
  - `symbol/` - Symbol library parsing and lookup

## CLI Commands

```bash
kicaddy config                    # Show detected KiCAD paths
kicaddy list-libraries [-v]       # List symbol libraries (verbose shows counts)
kicaddy list-symbols <path> [-v]  # List symbols in a .kicad_sym file
kicaddy dump-schematic <path>     # Parse and re-serialize schematic (round-trip test)
kicaddy new-schematic <path>      # Create empty schematic
kicaddy place-component <schematic> -l <library> -s <symbol> --x <x> --y <y> [-a angle] [-r ref] [-v value]
```

## Configuration

Set `KICAD_PATH` environment variable to KiCAD's shared support directory, or auto-detection will find standard locations.

macOS: `export KICAD_PATH="/Applications/KiCad/KiCad.app/Contents/SharedSupport"`

## Building

```bash
cargo build
cargo run -- config  # Test KiCAD detection
```

## Technical Notes

- **Grid alignment**: KiCAD schematics use a 50 mil (1.27mm) grid. The `place-component` command auto-snaps coordinates to this grid so pins align for wiring.
- **S-expression format**: KiCAD files use S-expressions. Order matters for some elements (e.g., pin `hide` must come before `name`).
- **lib_symbols**: When placing components, symbol definitions are embedded in the schematic's `lib_symbols` section with the full `Library:Symbol` name.
