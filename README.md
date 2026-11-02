# Strata

A native Linux resource monitor built in Rust with [GPUI](https://gpui.rs/). CPU, memory, system processes, and NVIDIA GPU telemetry live together in a borderless, resizable workspace.

- Stacked 60-second CPU history, 24 distinct colors, an adaptive column-major per-core legend, CPU model, and gradient-colored temperature.
- RAM segments for used memory, cache, buffers, and free memory; an aligned swap bar in the same compact summary.
- GPU and VRAM history with compact side-by-side meters and NVTOP-ordered device, clock, temperature, fan, power, PCIe, traffic, and encoder/decoder details above the graph.
- Network RX/TX history with a synchronized vertical scale, aggregate totals, and segmented per-interface rates. TCP/UDP socket counts appear in system process rows.
- Cached, virtualized system and GPU process tables. Click column headings to sort; click a row for its full command, user, state, thread count, CPU and memory details. Click a PID to copy it; the detail view also copies the complete command. Graphics/compute entries are merged by PID.
- Always-on panel dragging, height resizing, mixed-width rows, one to three columns, panel hiding, portrait/landscape layouts, and named presets.
- An Ubuntu-native light interface with compact controls, ruled system graphs, dense process tables, and responsive bordered panels.
- Timestamped, bounded graph smoothing, independent polling and animation rates, and pause/resume that preserves history and excludes paused wall time.

## Build and run

Requires Linux, a current stable Rust toolchain, and Vulkan-capable graphics drivers. NVIDIA monitoring uses the NVML library supplied with the proprietary NVIDIA driver; the monitor still opens without NVML. AMD telemetry is not implemented yet.

On Ubuntu 24.04:

```sh
sudo apt install build-essential pkg-config libfontconfig1-dev libfreetype6-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libxcb1-dev \
  libvulkan-dev libssl-dev
cargo run --release --locked
```

GPUI supports both Wayland and X11. The UI requests Noto Sans to match the surrounding GNOME desktop; Fontconfig supplies a fallback if it is absent.

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run --release --locked -- --check   # Live sampler diagnostic, no window
```

`--check` takes two samples one second apart, then prints the live CPU/GPU readings. No elevated privileges are needed to run Strata. Read permissions and hardware/driver support determine which readings are available; unavailable readings show `—`.

## Install as a desktop app

```sh
./scripts/install.sh
```

Installs the optimized binary to `~/.local/bin/strata`, the desktop entry to `~/.local/share/applications`, and the SVG icon to the user's icon theme. Launch **Resource Monitor** from the application menu. The desktop entry uses an absolute executable path, so the launcher does not depend on your shell's PATH. Supports `STRATA_PREFIX` for the binary prefix and the standard XDG data/config directories.

```sh
./scripts/package.sh                    # Portable archive for compatible Linux systems
./scripts/install.sh --no-build         # Install an already-built binary / extracted archive
./scripts/uninstall.sh                 # Remove app files, retain layouts
```

The archive is built for the host's architecture and libc; it is not a universal Linux binary.

## Use

Drag any panel header onto another panel to move it before that panel, anywhere in the workspace. Panel heights automatically fit the window while preserving readable minimums. Drag a panel's bottom grip to adjust its share of the available space. Choose one, two, or three columns in **Layout**. The **Full width / 1 col / 2 cols** control on each panel cycles its column span, allowing full-width graphs above side-by-side panels. A row shares its height across panels. Use the small `×` on a panel to hide it and **Layout** to show it again. While dragging, the drop target at the bottom of each column accepts panels too.

**Layout** contains portrait/landscape presets, saved workspaces, polling rates, animation rates, and font-size controls. Choose **Save preset**, type a name, and press Enter. Saving the same name replaces that preset. The `×` beside a saved preset deletes it. Every layout edit also saves the current workspace automatically to `$XDG_CONFIG_HOME/strata/settings.json` (normally `~/.config/strata/settings.json`). Invalid settings produce a visible message and fall back to defaults.

The default is a single column with CPU, memory, and system processes above the GPU and GPU-process panels. The grid reduces its effective column count when there is less than 480 logical pixels per column. Portrait presets explicitly use one column at any width. Panels remain on the same page; long workspaces and process lists scroll independently.

- `/` or **Filter**: type a process name, command fragment, username, or PID. Applies to both process lists. Enter finishes editing; Escape clears the filter.
- Space or **Pause/Resume**: freeze/resume measurements and graphs without clearing their history.
- Drag the empty top strip to move the window. Top controls minimize, maximize, and close it; the bottom corner grip resizes it. Desktop shortcuts such as Super+drag and Alt+F4 also work.

## Reading the measurements

CPU layers are each core's busy percentage divided by the number of logical cores. Their sum represents total machine utilization on a 0–100% scale. Numeric core readings retain their own 0–100% scale. Process CPU uses htop-style accounting: 100% represents one logical core, so multithreaded processes can exceed 100%.

RAM segments are disjoint: cache is `Cached + SReclaimable − Shmem`, buffers are `Buffers`, free is `MemFree`, and used is the remainder. “Used” includes unreclaimable kernel and shared memory; it is not the kernel's separate `Active` counter. All segments sum to physical RAM. The sampler diagnostic additionally reports pressure-oriented used memory (`MemTotal − MemAvailable`). Units are binary MiB/GiB.

The graphs display a one-poll delay so the newest segment can interpolate between real samples. Numeric readings show the latest measurement. Pausing freezes displayed data while the sampler stays warm; resume keeps existing graph samples and removes paused time from the graph clock. Changing polling rate may temporarily shift the delayed endpoint.

GPU memory is allocated VRAM, not memory-controller activity. GPU process utilization is driver-sampled SM activity; NVML may omit a process's utilization even when it reports that process's VRAM. Such readings show `—`. Per-process VRAM is counted once when the same PID appears in both graphics and compute lists. PCIe rates are converted from NVML's KiB/s to MiB/s. Temperature is the maximum readable CPU hwmon sensor from supported CPU drivers, not an average across cores.

Network totals sum non-loopback interfaces. Virtual bridges and physical interfaces can observe the same traffic; per-interface readings make this distinction visible. TCP/UDP columns count unique owned socket inodes in the monitor's network namespace, refreshed every three seconds. Unreadable process descriptors show `—`. These are socket counts, not per-process byte rates; reliable per-process bandwidth attribution needs a future tracing backend.

## Implementation

`src/metrics.rs` owns the sampler thread and Linux/NVML adapters. It publishes the latest snapshot through a single-slot mailbox; the UI never waits for sampling and no subprocess is launched for a poll. `/proc` CPU deltas exclude guest double-counting and account for PID reuse. NVIDIA is loaded dynamically through `nvml-wrapper`.

`src/chart.rs` keeps only compact graph samples in a bounded history. Charts share one animation clock at 30 fps by default (15/30/60 selectable), maintaining the selected rate when unfocused. Offscreen chart painting is skipped. Network and GPU lines use bounded, anti-aliased Bézier curves; CPU layers use bounded Catmull–Rom subdivisions while still emitting triangles directly, avoiding per-frame polygon tessellation and visual overshoot. Process rows are virtualized with GPUI's `uniform_list`, inside cached views that avoid rebuilding unrelated tables for chart animation. Release builds use thin LTO and stripped symbols.

`src/layout.rs` owns validated, serializable workspace state and atomic file replacement. `src/ui.rs` renders the panels and interactions. `src/theme.rs` owns shared colors and the vector icon family; `DESIGN.md` records the visual conventions. To add AMD support, supply the same normalized `Gpu`/`Process` readings through a separate driver adapter; the charts and panels are vendor-independent. NVIDIA is the only implemented GPU sampler in this version.

GPUI is GPU-accelerated and does allocate its own host and GPU memory. Lightweight does not mean terminal-sized overhead; use the optimized release build when evaluating CPU usage and responsiveness.
