#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
if [[ "${1:-}" != "--no-build" ]]; then
  cargo build --release --locked
fi
bin_dir="${STRATA_PREFIX:-$HOME/.local}/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm755 target/release/strata "$bin_dir/strata"
install -Dm644 assets/io.strata.Monitor.svg "$data_dir/icons/hicolor/scalable/apps/io.strata.Monitor.svg"
python3 - "$bin_dir/strata" "$data_dir/applications/io.strata.Monitor.desktop" <<'PY'
import pathlib, sys
binary = sys.argv[1].replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%')
entry = pathlib.Path('packaging/io.strata.Monitor.desktop').read_text().replace('Exec=strata', f'Exec="{binary}"')
p = pathlib.Path(sys.argv[2]); p.parent.mkdir(parents=True, exist_ok=True); p.write_text(entry)
PY
if command -v update-desktop-database >/dev/null; then update-desktop-database "$data_dir/applications"; fi
if command -v gtk-update-icon-cache >/dev/null; then gtk-update-icon-cache -f -t "$data_dir/icons/hicolor" >/dev/null 2>&1 || true; fi
printf 'Installed Resource Monitor. Launch it from your application menu or %s/strata\n' "$bin_dir"
