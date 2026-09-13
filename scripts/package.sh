#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
cargo build --release --locked
version="$(target/release/strata --version | awk '{print $2}')"
name="strata-$version-linux-$(uname -m)"
mkdir -p "dist/$name/target/release" "dist/$name/assets" "dist/$name/packaging" "dist/$name/scripts"
install -m755 target/release/strata "dist/$name/target/release/strata"
cp assets/io.strata.Monitor.svg "dist/$name/assets/"
cp packaging/io.strata.Monitor.desktop "dist/$name/packaging/"
cp scripts/install.sh scripts/uninstall.sh "dist/$name/scripts/"
cp README.md DESIGN.md LICENSE "dist/$name/"
tar -C dist -czf "dist/$name.tar.gz" "$name"
printf 'Created dist/%s.tar.gz. Extract and run scripts/install.sh --no-build.\n' "$name"
