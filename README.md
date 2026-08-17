# kicaddy

A Rust library and CLI for manipulating KiCAD schematic files, with a YAML-based
schematic compiler on top.

## Theory

KiCAD schematic files (`.kicad_sch`) are S-expression documents that describe
component instances, their library symbol references, wires, labels, and
hierarchical sheet structure. They are designed to be edited inside the KiCAD
GUI — writing them by hand or generating them from a script is painful because:

- Every placed component must embed the full library symbol definition in the
  schematic's `lib_symbols` section.
- Pin coordinates depend on symbol geometry, rotation, and mirroring.
- Wires must land on the 50 mil (1.27 mm) grid for KiCAD to consider pins
  electrically connected.
- Element ordering matters in some cases (e.g. `hide` must precede `name` on a
  pin).

`kicaddy` solves this in two layers:

1. **Low-level primitives** (`libkicaddy::schematic`, `libkicaddy::parser`) —
   a faithful S-expression parser/serializer and typed schematic model, plus
   commands to place components, route wires, add labels, and edit existing
   schematics. Coordinates auto-snap to the KiCAD grid.

2. **YAML schematic compiler** (`libkicaddy::yaml`) — a higher-level
   description format where you list components by symbol name, declare
   connections as named nets between pins, and let the compiler resolve symbols
   from your installed KiCAD libraries, lay components out, route wires, and
   emit a valid `.kicad_sch`. Hierarchical sheets, net scoping, and
   pullup/pulldown/decap detection are handled automatically.

A schematic written as a few hundred lines of YAML compiles to a working KiCAD
project that opens in the GUI for review or manual touch-up.

## Install / build

```bash
cargo build --release
export KICAD_PATH="/Applications/KiCad/KiCad.app/Contents/SharedSupport"  # macOS
./target/release/kicaddy config
```

`KICAD_PATH` is auto-detected on standard installs; set it explicitly if
detection fails.

## Examples

### Compile a YAML schematic

```bash
kicaddy compile kato-turntable-controller.yaml
```

Input is a YAML file describing components grouped by function and connections
declared as named nets:

```yaml
groups:
  Power Supply:
    components:
      U_LDO:
        symbol: Regulator_Linear:AP2112K-3.3
        value: AP2112K-3.3
      C_LDO_IN:
        symbol: Device:C
        value: 10uF
      C_LDO_OUT:
        symbol: Device:C
        value: 10uF
    connections:
      - net: 5V
        pins: [U_LDO:VIN, C_LDO_IN:1]
      - net: 3V3
        pins: [U_LDO:VOUT, C_LDO_OUT:1]
      - net: GND
        pins: [U_LDO:GND, C_LDO_IN:2, C_LDO_OUT:2]
  - no_connect: [U_LDO:EN]

```
$ kicaddy bom kato-turntable-controller.yaml
Bill of Materials
=================

Project: DCC Kato Turntable Controller
Revision: 1.0

 Qty  Description                              Value           References
 ---  -----------                              -----           ----------
   7  Device:C                                 100nF           C_CAN, C_CT, C_EN, ... (7 total)
   3  Device:C                                 10uF            C_ESP3, C_LDO_IN, C_LDO_OUT
   1  Device:C_Polarized                       470uF           C_BULK
   5  Device:R                                 10k             R_BOOT, R_CT_BOT, R_CT_TOP, R_EN, R_SW
   1  Espressif:ESP32-C6-WROOM-1               ESP32-C6-WROOM-1 U_ESP
   ...
```

### Netlist

Show every net a component sits on:

```
$ kicaddy netlist kato-turntable-controller.yaml -f U_LDO
&3V3: U_LDO:VOUT
&5V:  U_LDO:VIN
&GND: U_LDO:GND
```

### Semantic outline

Reads an existing `.kicad_sch` and summarises components and their pin
descriptions — useful for feeding a schematic to an LLM or auditing a board:

```
$ kicaddy outline kato-turntable-controller.kicad_sch
Components:
  Connector:USB_C_Receptacle_USB2.0_16P
    Instances: J_USB=USB-C
    Pins: CC1 CC2 D+ D- GND SBU1 SBU2 SHIELD VBUS
    Label: USB 2.0-only 16P Type-C Receptacle connector
  Diode:1N5819
    Instances: D_PWR_DCC D_PWR_USB D_BUCK
    Pins: A K
    Label: 40V 1A Schottky Barrier Rectifier Diode, DO-41
  ...
```

### Other commands

`kicaddy --help` lists the full set; the highlights:

- `new-schematic`, `place-component`, `add-wire`, `add-label`,
  `update-component`, `delete-component` — direct schematic editing.
- `search`, `symbol-info`, `list-libraries`, `list-symbols` — explore the
  installed KiCAD symbol libraries.
- `init-yaml` — scaffold a YAML schematic (`basic`, `regulator`, `led`).
- `print-layout` — print computed layout positions for debugging.
- `mcp` — start an MCP server so an AI assistant can drive the same commands.

## License

MIT — see [LICENSE](LICENSE).
