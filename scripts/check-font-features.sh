#!/usr/bin/env bash
# Feature isolation checks for the bundled fonts
# (docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md, step 8).
#
#   scripts/check-font-features.sh [--quick]
#
# 1. `cargo check` with and without default features, for each font
#    feature and group alone, and for wasm32 and aarch64-linux-android with
#    no font feature and with all of them.
# 2. The byte probe: `cargo build --release --lib` per configuration, then a
#    search of the produced rlib and rmeta for a 64-byte window of every
#    bundled TTF and atlas (the first window with at least 32 distinct byte
#    values). With no font feature no window may be found; with all font
#    features every window must be found, and with --no-default-features and
#    all font features every atlas window and no TTF window.
# 3. Determinism: the committed atlases are re-baked in memory and compared
#    (`libmsdf-bake bundled --check fonts/atlases`).
#
# --quick skips the cross-target checks and the determinism bake.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0
QUICK=${1:-}
FEATURES=(font-apache-roboto-regular font-apache-roboto-bold font-apache-roboto-italic
          font-apache-roboto-msymbols font-ofl-grenze-gotisch font-ofl-texturina
          font-ofl-noto-sans-mono fonts-apache-roboto fonts-ofl-ledger)
ALL=fonts-apache-roboto,fonts-ofl-ledger,font-ofl-noto-sans-mono

step() { printf '\n== %s\n' "$*"; }

step "cargo check (default and no default features)"
cargo check -q
cargo check -q --no-default-features
for f in "${FEATURES[@]}"; do
  step "cargo check --features $f (with and without default features)"
  cargo check -q --features "$f"
  cargo check -q --no-default-features --features "$f"
done

if [[ "$QUICK" != "--quick" ]]; then
  for target in wasm32-unknown-unknown aarch64-linux-android; do
    step "cargo check --target $target (no font feature, all font features, both default settings)"
    cargo check -q --target "$target" --lib
    cargo check -q --target "$target" --lib --no-default-features
    cargo check -q --target "$target" --lib --features "$ALL"
    cargo check -q --target "$target" --lib --no-default-features --features "$ALL"
  done
fi

probe() { # $1 = label; rest = cargo args
  local label=$1; shift
  cargo build -q --release --lib "$@"
  local dir="${CARGO_TARGET_DIR:-target}/release/deps"
  local files=("$dir"/liblibmsdf-*.rlib "$dir"/liblibmsdf-*.rmeta "$dir"/liblibmsdf_font-*.rlib "$dir"/liblibmsdf_font-*.rmeta)
  python3 - "$label" "${files[@]}" <<'PY'
import glob, pathlib, sys
label, files = sys.argv[1], sys.argv[2:]
blobs = [pathlib.Path(f).read_bytes() for f in files if pathlib.Path(f).exists()]
def window(data):
    for i in range(0, len(data) - 64):
        w = data[i:i + 64]
        if len(set(w)) >= 32:
            return w
    raise SystemExit("no window")
found = {}
for p in sorted(glob.glob("fonts/*.ttf")) + sorted(glob.glob("fonts/atlases/*.atlas")):
    w = window(pathlib.Path(p).read_bytes())
    found[p] = any(w in b for b in blobs)
print(label)
for p, f in found.items():
    print(f"  {'FOUND' if f else 'absent'}  {p}")
pathlib.Path(f"out/fonts/probe-{label}.txt").write_text("\n".join(f"{f} {p}" for p, f in found.items()))
PY
}

mkdir -p out/fonts
step "byte probe: no font feature"
probe none
step "byte probe: all font features"
probe all --features "$ALL"
step "byte probe: all font features, no default features"
probe all-no-default --no-default-features --features "$ALL"
python3 - <<'PY'
import pathlib
def load(n):
    return {l.split(" ", 1)[1]: l.split(" ", 1)[0] == "True"
            for l in pathlib.Path(f"out/fonts/probe-{n}.txt").read_text().splitlines()}
none, allf, nodef = load("none"), load("all"), load("all-no-default")
bad = [p for p, f in none.items() if f]
assert not bad, f"font bytes in a build with no font feature: {bad}"
assert all(allf.values()), f"positive control failed: {[p for p, f in allf.items() if not f]}"
for p, f in nodef.items():
    want = p.endswith(".atlas")
    assert f == want, f"--no-default-features with all fonts: {p} found={f}, want {want}"
print("byte probe: OK")
PY

if [[ "$QUICK" != "--quick" ]]; then
  step "determinism: re-bake the committed atlases"
  cargo run -q -p libmsdf-font --features bake --bin libmsdf-bake -- bundled --check fonts/atlases
fi
step "OK"
