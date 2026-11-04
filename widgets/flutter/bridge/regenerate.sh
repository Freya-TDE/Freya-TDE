#!/usr/bin/env bash
# Erzeugt die Bindungen von flutter_rust_bridge neu, nachdem api.rs oder die
# Version geändert wurde. / Regenerates the flutter_rust_bridge bindings after
# api.rs or the version changed.
#
# Braucht / needs: flutter, flutter_rust_bridge_codegen (gleiche Version wie
# in pack.toml / same version as in pack.toml).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
version="$(grep -oP 'crate = "flutter_rust_bridge", version = "=\K[0-9.]+' "$here/../pack.toml")"

mkdir -p "$work/src" "$work/flutter/lib"
cat > "$work/Cargo.toml" <<TOML
[package]
name = "freya_bridge"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
flutter_rust_bridge = "=$version"
TOML
cat > "$work/src/lib.rs" <<'RS'
pub mod api;
mod frb_generated;
mod app {
    use crate::api::*;
    pub fn freya_host_start() -> Result<(), String> { Ok(()) }
    pub fn freya_host_open(_: &str) -> Vec<FreyaControl> { Vec::new() }
    pub fn freya_host_snapshot(_: &str) -> Vec<FreyaControl> { Vec::new() }
    pub fn freya_host_event(_: &str, _: &str, _: &str, _: FreyaInput) -> Vec<FreyaControl> { Vec::new() }
    pub fn freya_host_poll(_: &str) -> Option<Vec<FreyaControl>> { None }
    pub fn freya_host_close(_: &str) -> bool { true }
}
RS
cp "$here/api.rs" "$work/src/api.rs"
cat > "$work/flutter/pubspec.yaml" <<YAML
name: freya_bridge_app
publish_to: none
environment:
  sdk: ^3.9.0
dependencies:
  flutter:
    sdk: flutter
  flutter_rust_bridge: $version
YAML
cp "$here/flutter_rust_bridge.yaml" "$work/flutter/"
(cd "$work/flutter" && flutter pub get && flutter_rust_bridge_codegen generate)
cp "$work/src/frb_generated.rs" "$here/frb_generated.rs"
cp "$work/flutter/lib/src/rust/"*.dart "$here/dart/"
echo "ok: bindings for flutter_rust_bridge $version"
