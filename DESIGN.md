# Strata interface

Strata is an operating tool for a Linux workstation, primarily on a portrait monitor. The visual direction is a quiet native instrument panel, informed by Zed. This document records the existing brief and the implemented polish pass; it is not a replacement product brief.

## Keep

- One typeface: JetBrains Mono for controls, commands, and measurements. A restrained 10–13 px role scale; process row sizing remains adjustable.
- Charcoal neutral surfaces. The workspace between panels is slightly transparent; panel contents remain opaque.
- No branded header or app title in the interface. A narrow top drag area contains ordinary window controls.
- CPU and memory remain separate panels. CPU-related panels precede the GPU group in the default portrait layout.
- Reordering and resizing are always available. Rows support mixed spans and automatically fit the window, with scrolling below readable minimum sizes.
- Real data, bounded histories, virtualized process lists, and no decorative animation.

## Visual vocabulary

Shared colors and authored vector icons live in `src/theme.rs`. Panel boundaries use a single subtle border and a 4 px radius; neither shadows nor nested decorative cards are used. A 12 px workspace gutter distinguishes movable panels. Content uses consistent horizontal insets.

Titles use ordinary sentence case, except standard acronyms such as CPU, GPU, RAM and PID. Counts and units sit beside the relevant measurement. Redundant labels such as CURRENT, TASKS, and NVIDIA / NVML do not belong in panel chrome. Backend details belong in diagnostics and documentation.

Icons share a 16-unit drawing grid and a 1.15 px stroke. Window controls, grip, copy, filter, layout, playback, and resize geometry use that family rather than font glyphs. Numeric arrows and percent signs remain measurement notation.

Tables align numeric data and headings to the right; commands and user names align to the left. PID is an interactive text cell that copies its value. A row opens a complete, scrollable command detail with an explicit copy action. Missing readings use an em dash, never fabricated zeroes.

CPU layers retain distinct colors because identifying cores is a user requirement. Memory colors retain stable meanings: used, cache, buffers, free. GPU/VRAM and network receive/send colors distinguish simultaneous series. Accent does not decorate empty space.

Panel width controls say “Full width”, “1 col”, or “2 cols”. One-column layouts do not expose a width control that cannot change anything. Destructive-looking close icons hide panels and are recoverable from Layout → Panels. Pause preserves history; graph movement represents time only.

## Verification scope

The shipped surface is GPUI on Linux, tested on X11/Xwayland at portrait and landscape sizes. This is not an HTML interface; web-only design detectors do not apply. Keyboard filtering, copying, dragging, preset persistence, and native window controls are part of the interaction contract. Full screen-reader accessibility is not claimed.
