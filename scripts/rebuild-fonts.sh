#!/usr/bin/env bash
# Rebuild and check every bundled TTF under fonts/ from its pinned upstream
# source, and optionally re-bake the bundled atlases.
#
#     scripts/rebuild-fonts.sh             # rebuild into a temp dir, compare
#     scripts/rebuild-fonts.sh --write     # ...and replace the rebuilt faces
#     scripts/rebuild-fonts.sh --atlases   # ...also bake every bundled atlas
#                                          #    and compare (several minutes)
#
# Upstream files come from scripts/fetch-upstream-fonts.sh (run first, with
# SHA-256 checks). The Python steps run under uv with fontTools pinned:
# `uv run --no-project --with fonttools==4.67.0 python3 -I`.
#
# Faces REBUILT from upstream, byte for byte (SOURCE_DATE_EPOCH pins
# head.modified to the committed value; nothing else in them is time-based):
#   GrenzeGotisch[wght]-subset.ttf, Texturina[opsz,wght]-subset.ttf
#       fonts/ofl_subset.py over google/fonts' files;
#   Roboto-Bold-ascii.ttf, Roboto-Italic-ascii.ttf
#       fonts/style.py over Roboto v2.138 (roboto-android.zip).
#
# Faces CHECKED, not rebuilt: Roboto-Regular-ascii.ttf and
# Roboto-Regular-ascii-msymbols.ttf are additive edits of a base face whose
# printable-ASCII half predates this repository, so they cannot be made from
# upstream alone. What is checked is that the committed faces are fixed
# points of the scripts that made them: fonts/widen.py (whose provenance
# check compares every ASCII outline and advance with Roboto v2.138),
# fonts/icon.py and, for the merged face, fonts/msymbols.py (whose check
# compares every borrowed icon with Material Symbols at the pinned commit)
# each return the face unchanged. fonts/marker.py refuses to run on a face
# that already has its markers, by design, so it is not re-run.
#
# --atlases runs `libmsdf-bake bundled --check fonts/atlases`, which bakes
# every atlas in fonts/bundled.sha256 in memory and compares it with the
# committed bytes (bit-reproducible on one host; the reference hashes are
# per host triple).
set -euo pipefail

write=0
atlases=0
for arg in "$@"; do
    case "$arg" in
        --write) write=1 ;;
        --atlases) atlases=1 ;;
        -h|--help) sed -n '2,35p' "$0"; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

root="$(cd "$(dirname "$0")/.." && pwd)"
fonts="$root/fonts"
up="$fonts/upstream"

command -v uv >/dev/null 2>&1 || { echo "this needs uv (https://docs.astral.sh/uv/) on PATH" >&2; exit 1; }

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

py() { # <SOURCE_DATE_EPOCH, or - for none> <script> <args>...
    local epoch="$1"
    shift
    if [ "$epoch" = - ]; then
        env -u SOURCE_DATE_EPOCH uv run --quiet --no-project --with fonttools==4.67.0 python3 -I "$@"
    else
        SOURCE_DATE_EPOCH="$epoch" uv run --quiet --no-project --with fonttools==4.67.0 python3 -I "$@"
    fi
}

status=0
compare() { # <built> <committed name> <expected sha256>
    local got
    got="$(sha256 "$1")"
    if [ "$got" != "$3" ]; then
        echo "FAIL: $2 rebuilt to SHA-256 $got, expected $3" >&2
        status=1
    elif ! cmp -s "$1" "$fonts/$2"; then
        echo "FAIL: fonts/$2 differs from the rebuild ($got)" >&2
        status=1
    else
        echo "ok: fonts/$2 is byte-identical to the rebuild ($got)"
    fi
}

"$root/scripts/fetch-upstream-fonts.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/rebuild-fonts.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# Rebuilt from upstream.
# committed name | upstream file | upstream sha256 | output sha256 [| instance]
OFL=(
    "GrenzeGotisch[wght]-subset.ttf|GrenzeGotisch[wght].ttf|701b299d8dc002a2b4bea2ff0f1272c0e4081a2835914354804565c410d0c637|19f01d3ccd8d1abcc29db790d3e94f68166797e05727dc798b3045086042f9e7"
    "Texturina[opsz,wght]-subset.ttf|Texturina[opsz,wght].ttf|478a15d7145cf94565cfc1aeb33596aeb0118c7d86a84d061ddf46de3d7dfda3|399fd6669e246d50d47ec89904fb4df9efea7597768663b391f3847f4614737e"
    "NotoSansMono-Regular-subset.ttf|NotoSansMono[wdth,wght].ttf|2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081|436c580a03128211bf11c4ef1cb10d2f5ab31473829ee4585f8ebdb75313c37e|wght=400,wdth=100"
    "NotoSansMono-Bold-subset.ttf|NotoSansMono[wdth,wght].ttf|2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081|b77c8dee8ec3fb5e01553b9d4323e1723dc686f77bd9eb7cd31918316c82a06f|wght=700,wdth=100"
)
for entry in "${OFL[@]}"; do
    IFS='|' read -r name src src_sha want instance <<<"$entry"
    # ofl_subset.py does not recalculate head.modified; no epoch is needed.
    # A fifth field makes a static instance first (the Noto Sans Mono subsets).
    py - "$fonts/ofl_subset.py" "$up/$src" "$work/$name" "$src_sha" \
        ${instance:+--instance "$instance"} >/dev/null
    compare "$work/$name" "$name" "$want"
done

# committed name | upstream style face | SOURCE_DATE_EPOCH | output sha256
STYLE=(
    "Roboto-Bold-ascii.ttf|Roboto-Bold.ttf|1788054955|f3ed9c770b5fd59cf8d40d4f8e70ae28a0ff5c96fc5271ae7a1653f0d2e6d5eb"
    "Roboto-Italic-ascii.ttf|Roboto-Italic.ttf|1788054956|c385f74a3e81baf200fddf547c5efdd367c6e6c21dc4989c4962d0302e062995"
)
for entry in "${STYLE[@]}"; do
    IFS='|' read -r name src epoch want <<<"$entry"
    py "$epoch" "$fonts/style.py" "$up/roboto-android/$src" "$work/$name" \
        "$up/roboto-android/Roboto-Regular.ttf" >/dev/null
    compare "$work/$name" "$name" "$want"
done

# Checked as fixed points.
# committed name | SOURCE_DATE_EPOCH (its head.modified) | sha256
FIXED=(
    "Roboto-Regular-ascii.ttf|1789684994|b7ba6ab7d6dca2d1611e5e3c287f30e40116758928e922b440ce8666db1d6a85"
    "Roboto-Regular-ascii-msymbols.ttf|1789684994|12f0fe674e25ebb3763c37e838c408f2fd128ce17c393d504e5bfe94e65237de"
)
for entry in "${FIXED[@]}"; do
    IFS='|' read -r name epoch want <<<"$entry"
    base="${name%.ttf}"
    py "$epoch" "$fonts/widen.py" "$fonts/$name" "$work/$base.widen.ttf" \
        "$up/roboto-android/Roboto-Regular.ttf" >/dev/null
    py "$epoch" "$fonts/icon.py" "$work/$base.widen.ttf" "$work/$base.icon.ttf" >/dev/null
    last="$work/$base.icon.ttf"
    if [ "$name" = "Roboto-Regular-ascii-msymbols.ttf" ]; then
        py "$epoch" "$fonts/msymbols.py" "$last" "$work/$base.msymbols.ttf" \
            "$up/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf" \
            "$up/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].codepoints" >/dev/null
        last="$work/$base.msymbols.ttf"
    fi
    compare "$last" "$name" "$want"
done

# Merged from several upstream faces (the terminal symbol page).
# committed name | script | output sha256
MERGED=(
    "LibmsdfTerminalSymbols-Regular.ttf|terminal_symbols.py|0d91232725e03887601f6ea857300ea9c384ab6b89b05e171060246966640cca"
)
for entry in "${MERGED[@]}"; do
    IFS='|' read -r name script want <<<"$entry"
    # terminal_symbols.py sets head.created and head.modified itself.
    py - "$fonts/$script" "$up" "$work/$name" >/dev/null
    compare "$work/$name" "$name" "$want"
done

if [ "$write" = 1 ] && [ "$status" != 0 ]; then
    for entry in "${OFL[@]}" "${STYLE[@]}" "${MERGED[@]}"; do
        IFS='|' read -r name _ <<<"$entry"
        cp "$work/$name" "$fonts/$name"
        echo "wrote fonts/$name (update its sha256 in font/src/bundled.rs and FONTS-LICENSE-*.txt)"
    done
fi

if [ "$atlases" = 1 ]; then
    (cd "$root" && cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- bundled --check fonts/atlases) || status=1
fi
exit "$status"
