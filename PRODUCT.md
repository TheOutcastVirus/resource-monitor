# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

## Users

Inferred from the application and the supplied redesign brief: Linux workstation users who need to inspect live CPU, memory, process, GPU, network, and disk-related activity while diagnosing performance or watching a workload.

## Product Purpose

Strata brings system and NVIDIA GPU telemetry into one responsive desktop monitor. Success means the user can recognize load, find an expensive process, and inspect the relevant detail quickly while the monitor itself remains lightweight.

## Positioning

Strata combines the familiar scanability of a native Linux system monitor with GPU telemetry and a configurable panel workspace that can be reordered, resized, hidden, and saved.

## Operating Context

Inferred from the repository and supplied screenshots: Strata runs as a native Linux desktop application on X11 or Wayland, often beside development or graphics workloads and on both portrait and landscape displays.

## Capabilities and Constraints

- Preserve CPU, memory and swap, system process, NVIDIA GPU and VRAM, GPU detail, GPU process, and network monitoring.
- Preserve sorting, filtering, process detail and copy actions, pause and resume, polling and chart frame-rate controls, movable and resizable panels, responsive one-to-three-column layouts, hidden panels, and saved presets.
- Keep sampling off the UI thread, histories bounded, charts GPU-rendered, and long process lists virtualized.
- Missing measurements remain explicit rather than being shown as zero.
- AMD GPU telemetry remains an open product decision.

## Brand Commitments

- Product name: Strata.
- The redesign should feel native to Ubuntu and closely follow the supplied Ubuntu System Monitor references in hierarchy, density, table treatment, graph language, and restrained chrome.
- The native reference is adapted to Strata's existing richer telemetry and configurable panel model.

## Evidence on Hand

- Working Rust and GPUI implementation in `src/`.
- Product behavior and technical notes in `README.md`.
- Ubuntu System Monitor screenshots supplied in the redesign request.
- Existing Strata application icon in `assets/io.strata.Monitor.svg`.
- No user research, accessibility study, performance benchmark, or external product claim has been supplied.

## Product Principles

- Read system state at a glance.
- Keep advanced telemetry discoverable without crowding routine monitoring.
- Preserve direct manipulation and user-owned layouts.
- Spend fewer resources than the workloads being inspected.
- Follow Linux desktop conventions and show unavailable data honestly.

## Accessibility & Inclusion

Inferred requirement: text, data series, selected rows, focus and hover states should remain distinguishable without depending on color alone. Full screen-reader support has not been established for the current GPUI surface.
