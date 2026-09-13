#!/usr/bin/env bash
set -euo pipefail
bin_dir="${STRATA_PREFIX:-$HOME/.local}/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f -- "$bin_dir/strata" "$data_dir/applications/io.strata.Monitor.desktop" "$data_dir/icons/hicolor/scalable/apps/io.strata.Monitor.svg"
printf 'Strata removed. Your layouts remain in %s/strata.\n' "${XDG_CONFIG_HOME:-$HOME/.config}"
