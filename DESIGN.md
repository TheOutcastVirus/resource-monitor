---
name: Strata
description: A calm Ubuntu-native instrument for dense, trustworthy system telemetry.
colors:
  primary: "#3584e4"
  workspace: "#f7f7f7"
  panel: "#ffffff"
  raised-control: "#f1f1f1"
  separator: "#d9d9d9"
  chart-grid: "#d8dadd"
  text: "#3d3846"
  text-muted: "#6f6b72"
  selection: "#e7e5ff"
  row-alternate: "#fafafa"
  row-hover: "#ececec"
  control-hover: "#e7e7e7"
  receive: "#1c71d8"
  transmit: "#c64600"
  gpu: "#2e7d4f"
  memory-used: "#2ec27e"
  memory-cache: "#9141ac"
  memory-buffer: "#3584e4"
  memory-free: "#d5d5d5"
  swap: "#c061cb"
  vram: "#7b3f94"
typography:
  title:
    fontFamily: "Noto Sans, Ubuntu, sans-serif"
    fontSize: "12px"
    fontWeight: 600
  body:
    fontFamily: "Noto Sans, Ubuntu, sans-serif"
    fontSize: "12px"
    fontWeight: 400
  measurement:
    fontFamily: "Noto Sans, Ubuntu, sans-serif"
    fontSize: "13px"
    fontWeight: 400
  label:
    fontFamily: "Noto Sans, Ubuntu, sans-serif"
    fontSize: "10px"
    fontWeight: 400
  micro:
    fontFamily: "Noto Sans, Ubuntu, sans-serif"
    fontSize: "9px"
    fontWeight: 400
rounded:
  marker: "1px"
  meter-sm: "2px"
  meter: "3px"
  compact-control: "6px"
  panel: "8px"
spacing:
  tight: "4px"
  compact: "8px"
  panel-gap: "10px"
  standard: "12px"
  panel-inset: "16px"
components:
  compact-button:
    backgroundColor: "{colors.raised-control}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.compact-control}"
    padding: "5px 10px"
  compact-button-active:
    backgroundColor: "{colors.raised-control}"
    textColor: "{colors.primary}"
    typography: "{typography.body}"
    rounded: "{rounded.compact-control}"
    padding: "5px 10px"
  icon-button:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.compact-control}"
    size: "28px"
  panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.panel}"
  panel-header:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    typography: "{typography.title}"
    height: "38px"
    padding: "0 12px"
  table-header:
    backgroundColor: "{colors.raised-control}"
    textColor: "{colors.text-muted}"
    typography: "{typography.label}"
    height: "26px"
    padding: "0 12px"
  status-bar:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text-muted}"
    typography: "{typography.label}"
    height: "28px"
    padding: "0 12px"
---

# Design System: Strata

## Overview

**Creative North Star: "The Calm Ubuntu Instrument"**

Strata should feel immediately at home on a contemporary Ubuntu desktop: pale neutral workspace, crisp white instruments, Noto Sans, compact controls, and quiet gray rules. The interface is an operating surface for sustained observation, so density, legibility, and stable visual meaning carry more weight than decoration.

Panels expose their configurability without turning the workspace into an editor. A small grip, a plain width control, a recoverable hide action, and a restrained resize edge remain visible as part of the instrument. Color is reserved for live series, measurements, selection, sorting, and active state.

Motion belongs to measured time. Graphs may move as samples advance; the rest of the interface remains still. Smooth curves stay within the values of adjacent samples, histories remain bounded, and missing telemetry is stated rather than visually fabricated.

**Key Characteristics:**

- Pale neutral workspace with white, thinly ruled panels.
- Compact GNOME-like controls and dense, readable data rows.
- Blue active and sort states; categorical color identifies telemetry.
- Visible but restrained grip, hide, span, and resize affordances.
- Real-time graph movement with bounded data and no decorative animation.

## Colors

The palette pairs warm-neutral Ubuntu work surfaces with a clear blue interaction accent and a disciplined set of telemetry colors.

### Primary

- **Ubuntu Action Blue** (`primary`): Active controls, current sort headings, CPU summary values, focused drop borders, and interactive PID hover states.

### Secondary

- **Receive Blue** (`receive`): Network receive series and TCP/UDP process values.
- **Transmit Orange** (`transmit`): Network transmit series and the paused-state emphasis.
- **Telemetry Green** (`gpu`, `memory-used`): GPU utilization and used-memory signals.
- **Telemetry Purple** (`memory-cache`, `vram`): Cache, VRAM, and related secondary telemetry.
- **Swap Magenta** (`swap`): Swap utilization only.

### Neutral

- **Pale Workspace** (`workspace`): The canvas visible between movable panels.
- **Instrument White** (`panel`): Panel bodies, title bar, status bar, and table rows.
- **Control Gray** (`raised-control`): Compact controls, table headings, and expanded layout settings.
- **Separator Gray** (`separator`): Panel outlines, header rules, and control borders.
- **Graph Paper Gray** (`chart-grid`): Ruled graph lines.
- **Ink** (`text`): Primary labels and values.
- **Secondary Ink** (`text-muted`): Supporting labels, units, summaries, and unavailable-state copy.
- **Lilac Selection** (`selection`): Selected process rows, paired with persistent content and interaction state.

The complete 24-color CPU series is normative in `.impeccable/design.json`. The legend always exposes every reported logical core, using the same indexed color as its stacked graph layer.

### Named Rules

**The Signal Color Rule.** Use color to identify a series, value, selection, sort, or active state; do not spend it on empty decoration.

**The Honest Absence Rule.** Render unavailable numeric measurements as an em dash and unavailable subsystems with explicit text such as “Not configured” or the reported error.

## Typography

**Display Font:** Noto Sans (with Ubuntu and generic sans-serif fallbacks)

**Body Font:** Noto Sans (with Ubuntu and generic sans-serif fallbacks)

**Character:** One familiar Linux desktop face carries the entire interface. Weight and a narrow size range establish hierarchy while keeping tables, controls, graph labels, and measurements visually related.

### Hierarchy

- **Measurement** (`measurement`): Prominent live values and application title moments.
- **Title** (`title`): Semibold panel names and emphasized telemetry labels.
- **Body** (`body`): Process rows, controls, and ordinary values; user settings may scale this role from 10 to 16 px.
- **Label** (`label`): Table headings, supporting summaries, status, and compact legends.
- **Micro** (`micro`): Axis labels and dense meter annotations only.

### Named Rules

**The Native Voice Rule.** Keep labels in sentence case and use standard uppercase only for established technical terms such as CPU, GPU, RAM, VRAM, PID, TCP, and UDP.

## Layout

The workspace uses one to three equal-weight columns with a 10 px gap and a 10 px outer inset. Each column requires roughly 480 px of usable width; the implementation collapses to one column below 984 px and cannot expose three effective columns below 1464 px. A configured span is clamped to the effective column count, so portrait layouts remain coherent without a separate mobile composition.

Panels flow in user-defined order and keep individual minimum heights. CPU, GPU, and network panels default to two-column spans in landscape; every panel uses one column in portrait. The scrollable workspace appears only when readable minimum heights exceed the window. Memory and GPU detail panels have restrained fitted maxima so compact summaries do not consume surplus height.

Panel configuration remains directly visible: drag from the header grip, cycle the width control when multiple columns are configured, hide from the header and restore from Layout → Panels, and resize from the 9 px lower edge with its centered 22 px rule. Keep these affordances visually quiet and spatially stable.

## Elevation & Depth

Strata uses no shadows. White panels separate from the pale workspace through tonal contrast, a one-pixel gray outline, and the workspace gutter. Table headers and compact controls use a slightly darker gray fill to show hierarchy without simulated elevation.

### Named Rules

**The Ruled Surface Rule.** Use tone, separators, and spacing to establish depth; do not add cards inside panels, drop shadows, blur, or translucency.

## Shapes

Panels use gently rounded 8 px corners and clip their ruled contents. Compact buttons and icon controls use 6 px corners. Meter tracks use 2–3 px rounding, while series markers remain nearly square. Borders stay one pixel and gray; shapes should feel like native controls rather than soft decorative capsules.

## Components

### Compact Controls

- **Shape:** 6 px corner radius with a one-pixel separator-colored border.
- **Default:** Raised-control gray behind ink text, with 10 px horizontal and 5 px vertical padding.
- **Hover:** Darken to the control-hover neutral.
- **Active:** Keep the same restrained surface and turn the label blue. Pause uses transmit orange when active because it changes live acquisition state.
- **Icon controls:** 28 px square, centered 14 px authored line icon, and the same hover neutral.

### Panels

- **Container:** Instrument-white background, 8 px corners, one-pixel separator outline, and no shadow.
- **Header:** Fixed 38 px height with 12 px horizontal inset, a grip, semibold title, optional count, flexible space, width control, and recoverable hide action.
- **Resize:** A 9 px interaction strip at the bottom with a centered 22 × 1 px rule; the strip takes the separator fill on hover.
- **Drag:** Use blue on the target border and a small neutral drag label. Do not animate panel movement.

### Process Tables

- **Header:** Fixed 26 px raised-control row with 10 px labels. The current sort label and its arrow are blue; hover darkens inactive headings to primary ink.
- **Rows:** Virtualized, single-line, and compact. Row height is the configured body size plus 13 px, with a very light alternate-row tone, bottom rules, ellipsis for overflow, and numeric alignment to the right.
- **Selection:** Lilac row fill plus retained content; hover uses a neutral gray. PID remains a separately interactive copy target.
- **Unavailable values:** Use an em dash. Empty states name whether data is loading, filtered out, unsupported, or unreadable.

### Ruled Telemetry Graphs

- **Frame:** One-pixel separator border with six horizontal intervals and six vertical time divisions in graph-paper gray.
- **Series:** Network, GPU, and VRAM use 1.8 px smooth strokes. CPU uses a stacked 24-color fill, normalized by core count so total height remains at or below 100%.
- **Legend:** Show the full logical-core legend in compact wrapping rows; do not collapse it to a subset.
- **Motion:** Advance only with real sampled time. Use a one-poll visual delay for interpolation, freeze the clock while paused, and retain a 62-second bounded history.
- **Rendering:** Smooth each CPU interval with clamped Catmull–Rom values and emit triangles directly. Do not introduce a chart library, per-frame polygon tessellation, unbounded history, or curve overshoot.

### Memory Meters

- **RAM:** A 12 px segmented bar whose stable order and colors are Used, Cache, Buffers, and Free.
- **Swap:** A 7 px magenta utilization bar; say “Not configured” when no swap total is present.
- **Labels:** Keep totals adjacent and the component compact enough to remain a summary panel.

### Application Chrome

- **Title bar:** 42 px instrument-white row with Strata at the left, a draggable center, compact Pause, Filter, and Layout controls, and authored minimize, maximize, and close icons.
- **Status bar:** 28 px instrument-white row with update or pause status and a restrained bottom-right resize icon.
- **Icons:** Use the single 16-unit, 1.15 px stroke family from the implementation; do not substitute emoji, text glyphs, or a mixed icon library.

## Do's and Don'ts

### Do:

- **Do** preserve the pale GNOME-like density, Noto Sans hierarchy, white panels, and thin gray separators.
- **Do** keep panel grip, width, hide, and resize affordances visible, restrained, and recoverable.
- **Do** preserve the full 24-core legend and its stable correspondence to the CPU graph layers.
- **Do** right-align numeric table data, left-align commands and users, and expose active sorting in blue with an arrow.
- **Do** show unavailable values explicitly with an em dash or specific status copy.
- **Do** keep graph histories bounded and smooth interpolation within the adjacent sampled values.

### Don't:

- **Don't** add decorative motion; real-time graph movement is the system's only continuous animation.
- **Don't** add shadows, glass effects, nested decorative cards, oversized type, or spacious marketing-page rhythms.
- **Don't** hide or simplify away the panel configurability that distinguishes Strata.
- **Don't** replace direct triangle emission for the CPU fill with a chart library or per-frame polygon tessellation.
- **Don't** fabricate zeroes for missing telemetry or let interpolation imply measurements that were never sampled.
