#!/usr/bin/env bash
# Fetch the upstream faces the bundled fonts are made from, at pinned
# sources, into the gitignored fonts/upstream/, checking every SHA-256.
#
#     scripts/fetch-upstream-fonts.sh
#
# What each one feeds (scripts/rebuild-fonts.sh runs them all):
#
#   GrenzeGotisch[wght].ttf, Texturina[opsz,wght].ttf
#       google/fonts at the commits named in FONTS-LICENSE-OFL.txt;
#       fonts/ofl_subset.py makes fonts/*-subset.ttf from them.
#   roboto-android/Roboto-{Regular,Bold,Italic}.ttf
#       the roboto-android.zip asset of googlefonts/roboto v2.138;
#       fonts/style.py makes Roboto-{Bold,Italic}-ascii.ttf from Bold and
#       Italic, and fonts/style.py and fonts/widen.py check the shipped
#       faces against Regular.
#   MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf
#       google/material-design-icons at a pinned commit; fonts/msymbols.py
#       checks the borrowed icons of Roboto-Regular-ascii-msymbols.ttf
#       against it outline for outline.
#
# Needs only curl, python3 and shasum or sha256sum. A file already present
# with the right SHA-256 is not downloaded again.
set -euo pipefail

GOOGLE_FONTS="https://raw.githubusercontent.com/google/fonts"
ROBOTO_ZIP_URL="https://github.com/googlefonts/roboto/releases/download/v2.138/roboto-android.zip"
ROBOTO_ZIP_SHA256="c825453253f590cfe62557733e7173f9a421fff103b00f57d33c4ad28ae53baf"
MSYMBOLS_COMMIT="49d4db35df873165d6bd6ba09b063c7dafbac2f4"

# file in fonts/upstream | URL | sha256
DIRECT=(
    "GrenzeGotisch[wght].ttf|$GOOGLE_FONTS/98931f43a8dcf8ca908949780e0d03a6e4195227/ofl/grenzegotisch/GrenzeGotisch%5Bwght%5D.ttf|701b299d8dc002a2b4bea2ff0f1272c0e4081a2835914354804565c410d0c637"
    "Texturina[opsz,wght].ttf|$GOOGLE_FONTS/f4a8ff8c122abc147e5fd19fc46cb2396b67b2bb/ofl/texturina/Texturina%5Bopsz%2Cwght%5D.ttf|478a15d7145cf94565cfc1aeb33596aeb0118c7d86a84d061ddf46de3d7dfda3"
    "MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf|https://raw.githubusercontent.com/google/material-design-icons/$MSYMBOLS_COMMIT/variablefont/MaterialSymbolsOutlined%5BFILL%2CGRAD%2Copsz%2Cwght%5D.ttf|77ff1c20896afc6f1abf7d57fc408234dfd021ab050b63e0442e71d6c9f0d6b8"
)
# member of roboto-android.zip | sha256
ROBOTO=(
    "Roboto-Regular.ttf|797e35f7f5d6020a5c6ea13b42ecd668bcfb3bbc4baa0e74773527e5b6cb3174"
    "Roboto-Bold.ttf|36f3709dea3e3ce3c6aedc058079e55980825f898f1e901d091c73c40de8bab1"
    "Roboto-Italic.ttf|5cd9337020d0b1593bae42dcca5cfc0fb86f5806ef469d66d09eae1e56ce346b"
)

root="$(cd "$(dirname "$0")/.." && pwd)"
up="$root/fonts/upstream"
mkdir -p "$up/roboto-android"

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

check() { # <file> <expected sha256> <label>
    local got
    got="$(sha256 "$1")"
    if [ "$got" != "$2" ]; then
        echo "FAIL: $3 SHA-256 is $got, expected $2" >&2
        exit 1
    fi
    echo "ok: $3 $got"
}

have() { # <file> <expected sha256>
    [ -f "$1" ] && [ "$(sha256 "$1")" = "$2" ]
}

work="$(mktemp -d "${TMPDIR:-/tmp}/fetch-upstream-fonts.XXXXXX")"
trap 'rm -rf "$work"' EXIT

for entry in "${DIRECT[@]}"; do
    IFS='|' read -r name url want <<<"$entry"
    if have "$up/$name" "$want"; then
        echo "ok: $name (present)"
        continue
    fi
    echo "downloading $url"
    curl -fsSL --retry 3 -o "$work/download" "$url"
    check "$work/download" "$want" "$name"
    mv "$work/download" "$up/$name"
done

need_zip=0
for entry in "${ROBOTO[@]}"; do
    IFS='|' read -r name want <<<"$entry"
    have "$up/roboto-android/$name" "$want" || need_zip=1
done
if [ "$need_zip" = 1 ]; then
    echo "downloading $ROBOTO_ZIP_URL"
    curl -fsSL --retry 3 -o "$work/roboto-android.zip" "$ROBOTO_ZIP_URL"
    check "$work/roboto-android.zip" "$ROBOTO_ZIP_SHA256" "roboto-android.zip"
    for entry in "${ROBOTO[@]}"; do
        IFS='|' read -r name want <<<"$entry"
        # Read the one member as data and write it to a path chosen here.
        python3 -I -c 'import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z, open(sys.argv[3], "wb") as out:
    out.write(z.read(sys.argv[2]))' "$work/roboto-android.zip" "$name" "$work/$name"
        check "$work/$name" "$want" "roboto-android/$name"
        mv "$work/$name" "$up/roboto-android/$name"
    done
else
    echo "ok: roboto-android/ (present)"
fi
