---
title: Atlas-carried layout tables and a shaper without rustybuzz
date: 2026-10-16
status: In implementation
covers: >
  A new atlas file generation (file-major 5) with a split version field
  (file-major, major, minor, patch) and version-gated parsing that still reads
  the v3 and v4 files in use today; a table directory carrying tagged tables,
  with a tag convention (first-capital tags are simplified borrowings of
  OpenType tables, all-capital tags are libmsdf's own); the layout tables
  `Head`, `Cmap`, `Hmtx`, `Gsub` and `Gpos`; a `LICS` licence table required
  for every baked face; a format rule that no virtual-machine (bytecode)
  table is ever carried; `AtlasShaper`, a pure-Rust shaper that lays out text
  from those tables alone and produces the same `ShapedRun` as `TextShaper`;
  a bake-time equivalence check against rustybuzz that refuses faces the
  tables cannot represent; and making `rustybuzz` and `ttf-parser` 0.25
  optional behind a default-on `shaper` feature.
sources:
  - >
    Owner (Tim), 2026-10-16, relayed by the coordinating agent: the goal (lay
    out the baked text ranges from an atlas alone, with no TTF and no
    rustybuzz/ttf-parser in the consumer's build, identical to rustybuzz) and
    decisions 1 to 7 of section 2.
  - >
    Owner, 2026-10-16, clarifications relayed while this plan was drafted: the
    version field follows semver from major 0, parsing is gated with `>`
    comparisons on the version and a newer file is refused (decision 1); no
    virtual-machine tables (decision 8); a `LICS` table and the all-capital
    tag convention (decision 9).
  - src/font/atlas.rs (ATLAS_MAGIC, ATLAS_VERSION = 3, ATLAS_VERSION_LAYERED
    = 4, ATLAS_HEADER_SIZE = 28, AtlasLayer, SetMetrics, FontAtlas::to_bytes /
    from_bytes, FontAtlasBuilder::add_shaped_ascii_of, add_shipped_coverage,
    add_styled_coverage, build)
  - src/font/glyph_table.rs (GlyphEntry, 32 bytes, advance_x in em as f32)
  - src/font/shaper.rs (ShapedGlyph, ShapedRun, TextShaper with default
    features lnum + pnum, the control-character drop, StyledShaper)
  - src/font/mod.rs (TEXT_RANGES, GlyphStyle, MAX_RAW_GLYPH_ID,
    styled_glyph_id, bundled faces)
  - src/drawlist/mod.rs (DrawList::push_shaped_text)
  - examples/bake_atlas.rs, scripts/bake-atlases.py,
    tests/fixtures/roboto-ascii-48.atlas (a v3 file)
  - Cargo.toml (rustybuzz 0.20.1 and ttf-parser 0.25.1 non-optional;
    cpu-bake = msdfgen + ttf-parser 0.18)
  - >
    ~/src/highbay/designs/genius/GENIUS_PLAN.md, around lines 215 to 260
    (read only): kerning in ASCII is real; Roboto's GPOS `kern` has exactly
    two pairs involving the space, (space, T) and (T, space), each -40/2048
    em, and shaping words separately reflows lines.
  - >
    Measurements made for this plan with fontTools and HarfBuzz (uharfbuzz
    0.56.3), section 3, on the bundled Roboto faces and on the tower defense
    game's two OFL faces, Grenze Gotisch and Texturina (variable TTFs in that
    game's assets, read only).
related:
  - docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md (the model for this
    plan's shape; no technical dependency)
  - >
    Consumers that must keep compiling with default features: libcontrols
    (git rev b375be3), the tower defense game (path dependency on its libmsdf
    submodule; it ships two atlases baked from Grenze Gotisch and Texturina at
    v3), tinytrowel, sigilweaver, highbay.
---

# Atlas-carried layout tables and a shaper without rustybuzz

## 1. Purpose

A consumer of libmsdf draws text in two halves: it shapes a string into
glyph ids and advances (`TextShaper`, which wraps rustybuzz over a TTF), then
draws the run from a baked MSDF atlas (`DrawList::push_shaped_text`). The
atlas already holds everything needed to draw, but nothing needed to lay out:
the consumer must also ship the TTF and build rustybuzz and ttf-parser.

For a face whose layout needs are small (a character map, ligatures, pair
kerning), that is a heavy dependency for little work. This plan makes the
atlas carry those needs as small tagged tables, flattened at bake time, and
adds `AtlasShaper`, a pure-Rust shaper that reads them. Its output is the
same `ShapedRun` as `TextShaper`'s, glyph for glyph and unit for unit, for
every string in the face's baked coverage; the bake proves that on a corpus
and refuses a face it cannot represent instead of producing an atlas that
lays text out wrongly.

The work also gives the file format a version field that can grow without
another forced re-bake, a licence table so an atlas carries its faces'
licence identity, and a rule that keeps executable font data out of the
format.

## 2. Decisions made by the owner (2026-10-16)

1. **Version field.** From the next file generation the 4-byte version field
   is four bytes: **file-major, major, minor, patch**. File-major continues
   the format's generation numbering, so the next generation is file-major 5.
   Within a file-major, major.minor.patch follows semver starting at major 0.
   While major is 0 a minor bump may change how the file is parsed. Parsing
   is version-structured: the reader compares the file's version with `>`
   thresholds to decide which fields and tables are present, so one reader
   handles every older version of its file-major. A file newer than the
   newest version the reader knows is refused with a clear error. Patch
   never changes parsing. The v3 and v4 files in use today stay readable.
2. **Tagged tables borrowed from OpenType in a simplified flavour.** Layout
   data is carried as tagged tables behind a small directory (tag, offset,
   length). A table borrowed from OpenType keeps OpenType's name with only the
   first letter capitalised (`Cmap`, `Gsub`, `Gpos`), meaning "libmsdf's
   simplified flavour, not OpenType's".
3. **`AtlasShaper`**: pure Rust, no font-parsing dependencies: cmap lookup,
   ligature substitution, advances and kerning, producing the same
   `ShapedRun` as `TextShaper`, so `DrawList::push_shaped_text` and its
   callers are unchanged. A trait or an enum lets callers use either shaper.
4. **Dependencies.** `rustybuzz` and `ttf-parser` 0.25 become optional behind
   a `shaper` feature that is on by default, so existing consumers are
   unaffected. Baking needs them, since the bake extracts the tables.
5. **Proof.** A bake-time check shapes a corpus with both shapers and requires
   identical glyph ids and positions; a face needing anything else fails the
   bake with a clear error instead of drawing wrongly.
6. **Versioning (Rule 5).** Decide whether this release is additive, and
   whether a new file version counts as a breaking change (section 4.10).
7. **Steps** cover the version field and directory, the bake extraction,
   `AtlasShaper`, the equivalence check, optional dependencies and the
   committed fixtures (section 5).
8. **No virtual-machine tables.** The atlas carries only plain data tables,
   never bytecode or anything a virtual machine interprets (section 4.4).
9. **A `LICS` table.** All-capital tags are libmsdf's own tables, unrelated
   to TTF/OTF. `LICS` records each face's licence as a fourcc and a 4-byte
   version (major, minor, patch, subpatch), mapped to the standard SPDX
   identifier (section 4.8).

## 3. Findings (measured 2026-10-16)

All measurements use fontTools for table contents and HarfBuzz (uharfbuzz
0.56.3) to confirm behaviour. rustybuzz 0.20.1 is a port of HarfBuzz and is
the authority for the bake check; HarfBuzz was used here only to predict
what that check will find.

"Text glyphs" means the glyphs the face's cmap gives for `TEXT_RANGES`
(printable ASCII, Latin-1 Supplement and the typographic ten: 201
codepoints). All five faces map all 201.

### 3.1 Layout features present

| Face | upem | GSUB features | GPOS features |
|---|---|---|---|
| Grenze Gotisch (variable, wght) | 1000 | aalt case ccmp dlig dnom frac liga lnum locl numr onum ordn pnum salt sinf ss01 subs sups tnum zero | kern mark |
| Texturina (variable, opsz wght) | 1000 | aalt case ccmp dnom frac liga lnum locl numr onum ordn sinf subs sups zero | kern mark mkmk |
| Roboto Regular ascii (bundled) | 2048 | liga lnum pnum | kern |
| Roboto Bold / Italic ascii (bundled) | 2048 | dnom frac liga lnum numr pnum | kern |

**None of them has `calt`**, and none has a legacy `kern` table, a `rvrn`
feature or an `rlig`/`clig` feature.

### 3.2 What actually fires on the baked text ranges

Features rustybuzz applies by default for horizontal Latin text (ccmp, locl,
rlig, liga, clig, calt, kern, mark, mkmk, rclt), plus `lnum` and `pnum`,
which `TextShaper` turns on by default, restricted to lookups that can match
text glyphs:

- **Ligatures (`liga`, lookup type 4).** Roboto: f+i, f+l, f+f+i, f+f+l (two
  lookups). Grenze and Texturina: f+f+i, f+f+l, f+f, f+i, f+l (one lookup
  each). In all three, each first glyph's ligatures are listed longest first.
- **Figures (`lnum`, `pnum`, type 1 single substitution).** All five faces
  substitute the digits (Grenze's `zero` becomes `zero.lf`, Roboto's digits
  become glyph 101 and up). `pnum` in Grenze touches no text glyph.
- **Contextual substitution in `ccmp` (type 6, chaining context).** Both OFL
  faces have `ccmp` chaining rules that turn `i`/`j` into dotless forms
  before a combining mark. Combining marks are not in `TEXT_RANGES`, so those
  rules cannot fire on baked text, and every Texturina `ccmp` rule is dead
  for the same reason. **Grenze has live ones:** a spacing accent (grave
  U+0060, dieresis U+00A8, macron U+00AF, acute U+00B4) becomes its `.case`
  form when it follows a capital letter or another accent, or precedes
  another accent. HarfBuzz confirms: Grenze shapes "T`" to `T`, `grave.case`
  and "``" to two `grave.case`; Texturina leaves both alone. One of these is
  ASCII (the grave accent). This is not `calt`, but it is contextual, and it
  is the one thing in these faces that a plain ligature-and-kerning table
  cannot express (open question 1).
- **`locl`** applies only under a named language system (Grenze, Texturina:
  AZE, CAT, KAZ, MOL, PLK, ROM, TAT, TRK, CRT, NLD). `TextShaper` sets no
  language, so `locl` never applies.
- **Kerning (`kern`, type 2 pair adjustment, formats 1 and 2).** One lookup
  per face. Value format: Roboto 0x0004 (X advance only); Grenze and
  Texturina 0x0044 (X advance plus an X advance device table, which is zero
  at the default instance with no ppem). The second glyph never has a value.
  Non-zero pairs among text glyphs (classes expanded): Roboto 434 (all
  within ASCII; range -336..50, including (space, T) and (T, space) = -40,
  confirming the highbay finding), Grenze 218 (56 within ASCII; -60..60),
  Texturina 1627 (561 within ASCII; -140..50). Lookup flags: Roboto 0,
  Grenze and Texturina IgnoreMarks; no text glyph is in GDEF class 3 (mark)
  in any face, so the flag never changes a result on baked text.
- **`mark`/`mkmk`** (types 4 and 6) position combining marks only. No text
  glyph is a mark, so they never fire.
- **Fractions.** `frac`/`numr`/`dnom` are applied only around U+2044
  FRACTION SLASH, which is not baked; "1/2" shapes to lining figures and a
  plain slash in every face.

### 3.3 Behaviours a table-driven shaper must copy

Probed with HarfBuzz, to be pinned against rustybuzz by the bake check:

- **Soft hyphen (U+00AD) is in `TEXT_RANGES` and is a default ignorable.**
  It is shaped as the space glyph with zero advance, and lookups skip over
  it: "f\u{AD}i" becomes the `fi` ligature followed by the zero-width space
  glyph, both with cluster 0; "T\u{AD} " kerns T against the space
  (Roboto: T advance 1182 instead of 1222); in Grenze "T\u{AD}`" still turns
  the grave into `grave.case`.
- **Unmapped controls are dropped by `TextShaper`, mapped ones are not.**
  `TextShaper` drops glyph 0 when its source character is a control
  character. Grenze's cmap maps U+0009, U+000A, U+000D, U+0010 to U+0014,
  U+001E, U+001F and U+007F to real glyphs (HT, LF, CR, ...); HarfBuzz gives
  HT a zero advance and it is kept. Texturina and Roboto map no control
  codepoint, so a tab shapes to glyph 0 and is dropped.
- **`.notdef` advances:** Grenze 527, Texturina 500, Roboto 908. Space: 159,
  210 and 508.

### 3.4 Virtual-machine data in the source faces

| Face | Bytecode / VM tables present | Glyphs with instructions |
|---|---|---|
| Grenze Gotisch | `prep` (TrueType control-value program) | 0 |
| Texturina | `prep` | 0 |
| Roboto Regular, Regular+msymbols, Bold, Italic (bundled) | none | 0 |

None has `fpgm`, `cvt `, `CFF ` or `CFF2`. Both OFL faces also carry `gasp`
(rasteriser flags, not bytecode). Nothing in `Cmap`, `Hmtx`, `Gsub` or
`Gpos` depends on any of these: they are hinting data for a TrueType
rasteriser, which an MSDF atlas never runs. The bake leaves all of them
behind, along with the variation tables (`fvar`, `gvar`, `HVAR`, `avar`,
`STAT`): an atlas is baked from the default instance.

### 3.5 Licence data in the source faces (`name` table)

| Face | ID 0 (copyright) | ID 13 / 14 (licence) |
|---|---|---|
| Grenze Gotisch | Copyright 2020 The Grenze Gotisch Project Authors (https://github.com/Omnibus-Type/Grenze-Gotisch) | SIL OFL 1.1, http://scripts.sil.org/OFL |
| Texturina | Copyright 2020 The Texturina Project Authors (https://github.com/Omnibus-Type/Texturina) | SIL OFL 1.1, https://scripts.sil.org/OFL |
| Roboto Bold, Italic ascii | Copyright 2011 Google Inc. All Rights Reserved. | Apache 2.0, http://www.apache.org/licenses/LICENSE-2.0 |
| Roboto Regular ascii, Regular+msymbols | Copyright 2011 Google Inc. All Rights Reserved. | **absent** (stripped by the subsetting) |

Neither OFL face declares a Reserved Font Name in its copyright line. The
merged Regular+msymbols face also contains Material Symbols glyphs under
Apache 2.0 (`fonts/LICENSE-MaterialSymbols.txt`), which its `name` table does
not mention. So the licence cannot be read from the face alone: the bake
takes it from an explicit flag and checks it against the `name` table where
the table has one (section 4.8).

### 3.6 Existing atlas files

The committed fixture `tests/fixtures/roboto-ascii-48.atlas` and both of the
tower defense game's atlases begin `68 62 66 61 03 00 00 00`: magic `hbfa`,
version 3 as a little-endian u32.

## 4. Design

### 4.1 The version field

Bytes 4..8 of every atlas are the version. Until now they were a
little-endian u32 (1 to 4 have existed; this build reads 3 and 4). From
file-major 5 they are four separate bytes:

```text
byte 4: file_major   (5 for this generation)
byte 5: major        (semver major, starts at 0)
byte 6: minor        (semver minor)
byte 7: patch        (semver patch; never changes parsing)
```

**Telling old from new.** A legacy u32 version `v < 256` written little-endian
is the bytes `[v, 0, 0, 0]`. So the reader reads byte 4 first:

- byte 4 is 3 or 4 and bytes 5..8 are all zero: a legacy v3 or v4 file, read
  exactly as today.
- byte 4 is 3 or 4 and any of bytes 5..8 is non-zero: refused ("legacy atlas
  version with non-zero high bytes; not written by any libmsdf").
- byte 4 is 1 or 2: refused as today (v1 has no magic and fails the magic
  check first; v2's set rows are 32 bytes).
- byte 4 is 5: the split version, below.
- byte 4 is 0 or above 5: refused ("atlas file-major N is not one this build
  reads; it was written by a newer libmsdf, or is not an atlas").

No legacy file can be mistaken for a split one: the legacy writer only ever
wrote 3 and 4, and file-major starts at 5.

**The first version of file-major 5 is 0.1.0.** Under semver with major 0,
patch releases may not change parsing and minor releases may; the first
layout of a new generation defines parsing, so it is a minor version. 0.0.x
is never written. (0.0.1 would leave no way to express "parsing changed"
before 0.1.0 except by a minor bump anyway, and makes a reader's
`> (0, 0)` threshold meaningless.)

**Acceptance.** A reader knows a newest version `K = (major, minor)` within
file-major 5 (this plan: `(0, 1)`). For a file with version
`F = (major, minor, patch)`:

- `(F.major, F.minor) > K`: refused ("atlas format 5:M.m.p is newer than this
  build reads (5:0.1); update libmsdf").
- `(F.major, F.minor) < (0, 1)`: refused ("atlas format 5:0.0.p was never
  written by libmsdf").
- otherwise accepted. Patch is ignored: two files that differ only in patch
  parse identically. A patch bump records a writer change that leaves the
  layout the same (for example, a corrected `Gpos` value for one face).

**Version-gated parsing.** Inside file-major 5 the reader computes
`let v = (F.major, F.minor);` and gates every field added after 0.1 on a `>`
comparison with the version before it was added, the way version-gated macro
structures are read:

```rust
// Present since 0.1: header, layers, sets, glyphs, directory, tables, pixels.
// A future 0.2 that adds a field X to the header would read it as:
let x = if v > (0, 1) { read_u32(&mut cur)? } else { X_DEFAULT };
```

Semver past major 0 (what a minor bump may and may not do from 1.0) is left
open (open question 6); this plan writes only 0.1.0.

**Public API.** In `src/font/atlas.rs`:

```rust
/// File-major of the generation this build writes.
pub const ATLAS_FILE_MAJOR: u8 = 5;
/// The newest (major, minor, patch) of ATLAS_FILE_MAJOR this build reads and
/// the one it writes.
pub const ATLAS_FORMAT_VERSION: AtlasFormatVersion =
    AtlasFormatVersion { file_major: 5, major: 0, minor: 1, patch: 0 };

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AtlasFormatVersion { pub file_major: u8, pub major: u8, pub minor: u8, pub patch: u8 }

/// What the version field of a file says, before anything else is parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtlasFileVersion {
    /// A pre-split file: 3 or 4 (`ATLAS_VERSION`, `ATLAS_VERSION_LAYERED`).
    Legacy(u32),
    /// File-major 5 and later.
    Split(AtlasFormatVersion),
}

impl AtlasFileVersion {
    /// Reads bytes 0..8 (magic and version). Errors exactly as listed above.
    pub fn of(data: &[u8]) -> Result<Self, &'static str>;
}

impl FontAtlas {
    /// The version this atlas was read at (`ATLAS_FORMAT_VERSION` for one
    /// built in memory).
    pub fn file_version(&self) -> AtlasFileVersion;
}
```

`ATLAS_VERSION` (3), `ATLAS_VERSION_LAYERED` (4) and `ATLAS_HEADER_SIZE` (28)
keep their values and become the legacy constants; their docs say so.

**The writer always writes 5:0.1.0 from this release on.** One writer path:
the content-directed choice between v3 and v4 ends, and every file has a
layer table even with one layer. Older libmsdf releases refuse these files
by name (they read only 3 and 4); section 4.10 deals with what that means
for versioning.

**The test table** (step 1), each row a test case calling `from_bytes` on a
minimal file with that version and checking the result and which sections
were read:

| Bytes 4..8 | Reader newest | Result | Sections read |
|---|---|---|---|
| `03 00 00 00` | 5:0.1 | accept (legacy v3) | 28-byte header, sets, glyphs, pixels; one layer measured from the cells |
| `04 00 00 00` | 5:0.1 | accept (legacy v4) | 28-byte header, layer count and table, sets, glyphs, pixels |
| `03 00 01 00` | 5:0.1 | refuse | none (non-zero high bytes on a legacy version) |
| `02 00 00 00` | 5:0.1 | refuse | none (as today) |
| `05 00 01 00` | 5:0.1 | accept | 36-byte header, layers, sets, glyphs, directory, tables, pixels |
| `05 00 01 07` | 5:0.1 | accept, parsed identically to `05 00 01 00` | as above |
| `05 00 00 03` | 5:0.1 | refuse | none (0.0.x never written) |
| `05 00 02 00` | 5:0.1 | refuse | none (newer minor) |
| `05 01 00 00` | 5:0.1 | refuse | none (newer major) |
| `06 00 00 00` | 5:0.1 | refuse | none (newer file-major) |
| `00 00 00 00` | 5:0.1 | refuse | none |
| `05 00 01 00` | hypothetical 5:0.2 reader with field X gated `v > (0, 1)` | accept | as 0.1, X takes its default |
| `05 00 02 00` | hypothetical 5:0.2 reader | accept | as 0.1, plus X |

The two hypothetical rows are a unit test of the gating helper
(`fn gate(v: (u8, u8), since: (u8, u8)) -> bool { v > since }` with a
test-only field), not of a real 0.2 format.

### 4.2 File layout at 5:0.1.0

All integers little-endian. Offsets are from the start of the file.

```text
offset  size
0       4    magic "hbfa"
4       4    version [5, 0, 1, 0]
8       4    width u32
12      4    height u32
16      4    num_glyphs u32
20      4    channels u32
24      4    set_count u32
28      4    layer_count u32          (1..=256, always present)
32      4    table_count u32          (0..=64)
36           [AtlasLayer   x layer_count]   4 bytes each (unchanged)
             [SetMetrics   x set_count]    36 bytes each (unchanged)
             [GlyphEntry   x num_glyphs]   32 bytes each (unchanged)
             [TableRecord  x table_count]  16 bytes each
             zero padding to a multiple of 4
             table bodies, in directory order, each starting at a multiple
             of 4 and zero-padded to one
             pixel data: width * height * channels * layer_count bytes
```

`ATLAS_HEADER_SIZE_V5: usize = 36`.

**Existing sections stay a fixed prefix; only new data is tabled.** The
alternative, turning layers, set metrics, glyph entries and pixels into
tables too, was considered and is not recommended:

- the fixed sections are what every atlas must have and what the GPU path
  reads; a directory lookup for them buys no flexibility, since an atlas
  without glyphs or pixels is not an atlas;
- their byte layouts are unchanged, so the v3/v4 and v5 readers share one
  routine for each section after the header, and the GPU upload code does not
  change;
- the pixel data stays the tail of the file, which is what lets a reader
  check its length the way it does today.

Tables are for what is optional or per face, and that is everything this
plan adds.

**TableRecord** (16 bytes):

```text
0   4  tag       [u8; 4], ASCII
4   1  face      GlyphStyle ordinal (0..=3) for a per-face table;
                 0xFF for a table that belongs to the whole atlas
5   1  flags     0 (reserved; a reader refuses non-zero)
6   2  reserved  0
8   4  offset    u32, from the start of the file, multiple of 4
12  4  length    u32, unpadded body length in bytes
```

Rules, each checked on read (a violation is a refusal naming the rule):

- records are sorted by `(face, tag)` ascending and no `(face, tag)` repeats;
- the first body starts at the padded end of the directory, each next body
  starts at `align4(previous offset + previous length)`, and the pixel data
  starts at `align4(last offset + last length)` (or at the padded end of the
  directory when there are no tables);
- padding bytes are zero;
- total table bytes are at most 16 MiB (`MAX_ATLAS_TABLE_BYTES`);
- a tag is either **first-capital** (`[A-Z][a-z0-9]{3}`, padded with spaces
  on the right if shorter) or **all-capital** (`[A-Z0-9]{4}`, same padding);
  anything else is refused (so OpenType's lowercase tags such as `cmap`,
  `fpgm`, `prep`, `cvt ` can never be carried);
- an all-capital tag that is a registered OpenType table tag (`BASE`,
  `CBDT`, `CBLC`, `CFF `, `CFF2`, `COLR`, `CPAL`, `DSIG`, `EBDT`, `EBLC`,
  `EBSC`, `GDEF`, `GPOS`, `GSUB`, `HVAR`, `JSTF`, `MATH`, `MERG`, `MVAR`,
  `STAT`, `SVG `, `VARC`, `VORG`, `VVAR`) is refused, so a raw OpenType table
  cannot pass as one of ours;
- a table with a tag this build does not know is **skipped**: its bounds are
  checked as above and its body is not read. A known tag whose body fails to
  parse is a refusal.

### 4.3 The tag convention

- **First-capital tags** (`Head`, `Cmap`, `Hmtx`, `Gsub`, `Gpos`) are
  simplified borrowings of the OpenType table with the same name in lower or
  upper case. They answer the same question as that table, in a flattened,
  libmsdf-specific layout restricted to what the bake supports. Nothing about
  OpenType's byte layout carries over.
- **All-capital tags** (`LICS`) are libmsdf's own tables, unrelated to any
  TTF/OTF table.

### 4.4 Format rule: no virtual-machine data

The atlas carries only plain data tables: fixed-layout records that a reader
looks up and copies. It never carries bytecode or anything interpreted by a
virtual machine:

- no TrueType hinting programs or their data (`fpgm`, `prep`, `cvt `,
  per-glyph instructions in `glyf`);
- no CFF/CFF2 charstrings (`CFF `, `CFF2`);
- no equivalent in libmsdf's own tables: no table defined by this plan or a
  later one contains a program, an expression or an opcode stream.

Enforcement: the bake never copies a source table, it only writes the
records defined here; the directory defines no tag for these tables, and
section 4.2's tag rules refuse their tags even if a file names them;
`AtlasShaper` performs table lookups only and never executes code from a
file. (OpenType's GSUB/GPOS lookups are data, not programs, and in any case
reach the atlas only as the flattened records below.)

### 4.5 `Head` and `Hmtx`

The owner's list names `Cmap`, `Gsub` and `Gpos`. Two more small per-face
tables are needed to match rustybuzz exactly, and are proposed here:

- **`Head`**, because shaped positions are in font units and need the face's
  units per em, the glyph used to hide default ignorables, and a record of
  which features the bake applied;
- **`Hmtx`**, because `GlyphEntry::advance_x` is an f32 fraction of an em.
  Rounding it back to integer font units happens to be exact for current
  sizes, but an integer table makes the equality hold by construction and
  covers glyphs that have no cell.

Every per-face table has `face` = that face's `GlyphStyle` ordinal. A face
that carries layout has all five of `Head`, `Cmap`, `Hmtx`, `Gsub`, `Gpos`
(an empty `Gsub` or `Gpos` has a zero count); a face with only some of them
is refused.

**`Head`** (8 + 4 × feature_count bytes):

```text
0   2  units_per_em    u16 (16..=16384)
2   2  hidden_glyph    u16: glyph a default ignorable becomes (the face's
                       space glyph); 0 = the face has no space glyph and
                       ignorables are deleted
4   2  feature_count   u16
6   2  reserved        0
8      feature tags    [u8; 4] x feature_count, ascending, unique: every GSUB
                       and GPOS feature the bake applied and the face has
                       (for Roboto Regular: kern, liga, lnum, pnum)
```

**`Hmtx`** (4 + 4 × count bytes): `count u32`, then records
`{ glyph u16, advance u16 }` sorted by glyph, unique. It holds every glyph
`AtlasShaper` can output for this face: glyph 0, `hidden_glyph`, every `Cmap`
glyph and the closure of `Gsub` outputs from them.

### 4.6 `Cmap`, `Gsub`, `Gpos`

Glyph ids in all tables are the face's **raw** glyph ids (at most
`MAX_RAW_GLYPH_ID`, 16383); `AtlasShaper` turns them into atlas addresses
with `GlyphStyle::styled_glyph_id` at the end, as `StyledShaper` does.

**`Cmap`** (4 + 8 × count): `count u32`, then
`{ codepoint u32, glyph u16, reserved u16 = 0 }` sorted by codepoint, unique,
glyph never 0. **Scope:** every codepoint in `TEXT_RANGES` that the face maps,
plus every C0/C1 control codepoint (U+0000..U+001F, U+007F..U+009F) that the
face maps (section 3.3: Grenze maps eleven, and `TextShaper` keeps them).
A codepoint outside the scope is looked up as unmapped (glyph 0).

**`Gsub`** is the substitution lookups the bake applied, in the order
rustybuzz applies them (for the default Latin shaper: one stage, lookup-list
index order, after `rvrn`, which none of the measured faces has; the bake
check confirms the order for each face):

```text
0   2  lookup_count u16
2   2  reserved 0
then lookup_count lookups, each:
0   4  feature tag [u8; 4] the lookup came from (first feature, if several)
4   1  kind         1 = single, 2 = ligature, 3 = context single (see below)
5   1  reserved 0
6   2  reserved 0
8   4  record_count u32
12     records, by kind:
  kind 1, 4 bytes:  { from u16, to u16 } sorted by from, unique
  kind 2, 20 bytes: { len u8 (2..=8), reserved u8, out u16, components [u16; 8]
                      (first len used, rest zero) }, grouped by components[0]
                      ascending; inside a group, in the font's order (first
                      match wins, as in HarfBuzz; the measured faces list
                      longest first)
  kind 3, 8 bytes:  { side u8 (0 = context glyph before, 1 = after),
                      reserved u8, context u16, input u16, out u16 }
                      sorted by (input, side, context), unique
```

Kind 3 exists for Grenze's spacing-accent rules (section 3.2) and is
**proposed, pending the owner's answer to open question 1**. It expresses
exactly a chaining-context rule with one input glyph, one context glyph on one
side, and a single substitution, flattened over the glyph classes. If the
owner declines it, the bake treats such rules as unsupported and Grenze fails
the bake (section 4.9).

Lookups are restricted to **reachable glyphs**: the `Cmap` glyphs and,
repeatedly, the outputs of carried rules whose inputs are reachable, until no
new glyph appears. Rules whose inputs (and, for kind 3, context) are not all
reachable are dropped. A lookup left with no records is dropped.

**`Gpos`** (4 + 8 × count): `count u32`, then
`{ left u16, right u16, x_advance i16, reserved u16 = 0 }` sorted by
`(left << 16) | right`, unique, x_advance never 0. The value is added to the
left glyph's advance, in **font units** (an integer, as rustybuzz returns),
summed over every carried kern lookup. Class pairs are expanded over
reachable glyphs. For the measured faces this is at most 1,627 pairs among
text glyphs (Texturina, about 13 KB) before the ligature and figure forms
are added.

Font units rather than ems are a refinement of decision 2 ("x advance
adjustment in em units"): rustybuzz returns integer font units and
`ShapedRun` is in font units, so carrying `Head.units_per_em` plus integer
adjustments makes the equality exact, where an f32 em value would need
rounding back. Open question 2 asks the owner to confirm.

**Limits.** At most 64 tables, 65,535 lookups per face, ligatures of 2 to 8
components, raw glyph ids up to 16383, units per em 16 to 16384. Each is a
named constant and a refusal on read and at bake.

### 4.7 In-memory types

New module `src/font/layout.rs`, compiled without the `shaper` feature, holds
the parsed tables and their byte codecs:

```rust
pub struct AtlasLayout { faces: Vec<FaceLayout> }           // one per style
pub struct FaceLayout {
    pub style: GlyphStyle,
    pub units_per_em: u16,
    pub hidden_glyph: u16,
    pub features: Vec<[u8; 4]>,
    pub cmap: Vec<(u32, u16)>,
    pub hmtx: Vec<(u16, u16)>,
    pub gsub: Vec<GsubLookup>,
    pub gpos: Vec<(u32, i16)>,          // key = (left << 16) | right
}
pub struct GsubLookup { pub feature: [u8; 4], pub rules: GsubRules }
#[non_exhaustive]
pub enum GsubRules {
    Single(Vec<(u16, u16)>),
    Ligature(Vec<Ligature>),
    ContextSingle(Vec<ContextSingle>),   // only if open question 1 is accepted
}
impl AtlasLayout {
    pub fn face(&self, style: GlyphStyle) -> Option<&FaceLayout>;
    pub fn styles(&self) -> Vec<GlyphStyle>;
}
```

`FontAtlas` gains two public fields:

```rust
/// Layout tables, if the atlas was baked with them.
pub layout: Option<AtlasLayout>,
/// One record per licence of a baked face (empty for a runtime atlas and
/// for a legacy file).
pub licences: Vec<LicenceRecord>,
```

and `FontAtlas::licences(&self) -> &[LicenceRecord]`.

### 4.8 `LICS`: licences

One `LICS` table per atlas (`face` = 0xFF), required in every **baked**
atlas (built by `FontAtlasBuilder`). Runtime atlases (`FontAtlas::empty`) and
legacy files have none.

**Is there a standard?** To this plan's knowledge there is no standard binary
licence code: OpenType carries licences only as free text (`name` IDs 13 and
14) plus embedding permissions (`OS/2.fsType`), which are not a licence
identity. The standard identifier for a licence is **SPDX** (`OFL-1.1`,
`CC0-1.0`, `Apache-2.0`). So the fourcc and 4-byte version below are
**invented here** as a compact key, and every record also carries the SPDX
identifier as the unambiguous name.

```text
0   2  count u16
2   2  reserved 0
then count records, each:
0   4  licence fourcc    [u8; 4], registry below
4   4  licence version   [major, minor, patch, subpatch]
8   1  styles            bitmask of GlyphStyle ordinals this record covers
                         (bit 0 Regular, 1 Bold, 2 Italic, 3 BoldItalic)
9   1  reserved 0
10  2  reserved 0
12     spdx       u16 length + UTF-8 bytes (an SPDX licence identifier)
       source     u16 length + UTF-8 (what is licensed: the family name,
                  e.g. "Roboto", "Material Symbols")
       copyright  u16 length + UTF-8 (the face's copyright line, name ID 0)
       rfn        u16 length + UTF-8 (Reserved Font Name; empty if none)
       zero padding to a multiple of 4
```

Strings are at most 4096 bytes; records are in bake order; at least one
record per style the atlas carries cells for.

**Registry** (`LICENCE_REGISTRY` in `src/font/licence.rs`):

| fourcc | version | SPDX |
|---|---|---|
| `OFL ` | 1.1.0.0 | `OFL-1.1` |
| `APAC` | 2.0.0.0 | `Apache-2.0` |
| `CC0 ` | 1.0.0.0 | `CC0-1.0` |
| `CCBY` | 4.0.0.0 | `CC-BY-4.0` |
| `MIT ` | 0.0.0.0 | `MIT` (unversioned) |
| `UFL ` | 1.0.0.0 | `Ubuntu-font-1.0` |

Adding a licence is one row in that table plus a test that the row
round-trips; each (fourcc, version) maps to exactly one SPDX identifier, and
each SPDX identifier to one (fourcc, version). The **bake** refuses a licence
not in its registry. A **reader** accepts any fourcc (a newer libmsdf may
know more) and exposes it raw with the SPDX string; it refuses a record whose
fourcc and version are in its registry but whose SPDX string disagrees.

```rust
pub struct LicenceRecord {
    pub fourcc: [u8; 4],
    pub version: [u8; 4],
    pub styles: Vec<GlyphStyle>,
    pub spdx: String,
    pub source: String,
    pub copyright: String,
    pub reserved_font_name: String,
}
```

**At bake.** `FontAtlasBuilder` gains
`add_licence(&mut self, styles: &[GlyphStyle], spdx: &str, source: &str) -> Result<(), String>`;
the copyright line is read from the face's `name` ID 0 and the Reserved Font
Name parsed from it (the text after "with Reserved Font Name"). `build()`
refuses when any style it bakes has no licence record, when the SPDX is not
in the registry, or when the face's `name` ID 13/14 names a different licence
than the one given (ID 14 URLs `scripts.sil.org/OFL` or `openfontlicense.org`
mean OFL-1.1, `apache.org/licenses/LICENSE-2.0` means Apache-2.0; a face with
no ID 13/14, such as the Regular Roboto subsets, is taken from the flag
alone). `bake_atlas` gains
`--licence <SPDX>=<source>[@<style>,...]` (repeatable; style defaults to
`regular`). For the bundled faces: `Apache-2.0=Roboto` for each style, plus
`Apache-2.0=Material Symbols` for the merged face.

The atlas records the licence identity and the copyright line, which OFL
requires to accompany a derived work. **It does not carry the licence text**:
the full licence text stays in the consumer's licence file shipped with the
application, as it does today (`fonts/LICENSE-*.txt` here).

### 4.9 Bake extraction and the equivalence check

New module `src/font/layout_bake.rs`, behind the `shaper` feature (it needs
ttf-parser 0.25 and rustybuzz, not msdfgen):

```rust
pub struct LayoutOptions {
    /// Optional features to apply beyond the defaults, e.g. *b"tnum".
    pub extra_features: Vec<[u8; 4]>,
}
#[non_exhaustive]
pub enum LayoutError {
    /// A lookup in an applied feature can fire on reachable glyphs and is not
    /// a kind the tables carry.
    UnsupportedLookup { feature: [u8; 4], lookup_index: u16, lookup_type: u16, why: &'static str },
    /// A reachable glyph is a mark (GDEF class 3).
    MarkReachable { glyph: u16 },
    /// A positioning value other than the first glyph's x advance is non-zero.
    UnsupportedPositioning { left: u16, right: u16 },
    /// The two shapers disagree on a corpus string.
    Mismatch { text: String, rustybuzz: ShapedRun, atlas: ShapedRun },
    GlyphIdTooWide(u16),
    TooLarge(&'static str),
    Face(String),
}
impl FaceLayout {
    /// Extract one face's tables and run the equivalence check on them.
    pub fn extract(face_data: &[u8], style: GlyphStyle, opts: &LayoutOptions)
        -> Result<FaceLayout, LayoutError>;
}
```

**Extraction**, with ttf-parser 0.25's GSUB/GPOS/GDEF access:

1. Features applied: rustybuzz's defaults for horizontal text (ccmp, locl,
   rlig, liga, clig, calt, kern, mark, mkmk, rclt, rvrn) plus lnum and pnum
   (`TextShaper`'s defaults) plus `opts.extra_features`, intersected with the
   face's features, under script `latn`, falling back to `DFLT`, with no
   language system (so `locl`'s language-specific lookups are not applied).
2. `Cmap` from `glyph_index` over the scope of section 4.6; `Head` from
   `head`, the space glyph and step 1; `Hmtx` from `glyph_hor_advance` over
   the closure.
3. GSUB lookups in rustybuzz order. Type 1 becomes kind 1; type 4 becomes
   kind 2; type 6 (and type 7 wrapping it) whose every rule either can never
   match reachable glyphs or is one-input, one-context-glyph, single
   substitution becomes kind 3 (if accepted) with the dead rules dropped.
   Any other type (2 multiple, 3 alternate, 5 context, 8 reverse chaining),
   or a type 6 rule outside that shape, that can match reachable glyphs is
   `UnsupportedLookup`. Iterate the closure until stable.
4. GPOS: pair adjustment (type 2, formats 1 and 2, through type 9) in `kern`
   becomes `Gpos` records; any other GPOS type in an applied feature that can
   match reachable glyphs is `UnsupportedLookup`; any reachable GDEF mark is
   `MarkReachable`.

**The check** then shapes every corpus string with `TextShaper` (with the
same features as step 1) and with an `AtlasShaper` over the extracted tables,
and requires equality of every `ShapedGlyph` field (glyph id, x advance,
x offset, y offset, cluster), the glyph count, `total_advance` and
`units_per_em`. The first difference is `Mismatch`, whose `Display` prints
the string and both runs. The corpus:

- every `Cmap` codepoint alone, and every ordered pair of `Cmap` codepoints
  (201 × 201 = 40,401 strings for the measured faces);
- every Gsub rule's input sequence (and context) with every `Cmap` codepoint
  before it and after it, and with U+00AD inserted between each two of its
  components;
- every `Gpos` pair with U+00AD between them;
- pangrams: "The quick brown fox jumps over the lazy dog", "Sphinx of black
  quartz, judge my vow", "Pack my box with five dozen liquor jugs", "Waltz,
  bad nymph, for quick jigs vex", "Falsches Üben von Xylophonmusik quält
  jeden größeren Zwerg", "Portez ce vieux whisky au juge blond qui fume";
- ledger-style strings (constant `LAYOUT_CHECK_LEDGER`): "Treasury 50G",
  "Population 5", "fifty florins", "affluent office baffle", "flagon of
  fine fish", "To the Toll", "T T T", "Tax: +12G", "Total – 1,250G",
  "“Summa” … 100%", "½ ¼ ¾ 1/2", "AVA Ty To Te", "`T` ´A´ ¨E¨ ¯O¯",
  "line\nbreak\ttab\r", "soft\u{AD}hyphen of\u{AD}fice";
- strings with no letter at all (digits and punctuation only), which
  rustybuzz shapes under the `DFLT` script rather than `latn`.

The check runs every time a layout is extracted, so a face that needs
anything the tables cannot carry cannot produce an atlas. Expected results:
the three Roboto faces and Texturina pass; Grenze passes if kind 3 is
accepted and otherwise fails with `UnsupportedLookup { feature: ccmp, .. }`
on its lookups 42 and 45.

**At bake.** `FontAtlasBuilder::with_layout(&mut self, opts: LayoutOptions)`
asks `build()` to extract a `FaceLayout` for every face it bakes (regular and
each style) and to fail the bake on any `LayoutError`. Cells: every glyph in
the closure that `add_shaped_ascii_of` does not already queue (for example a
Grenze `grave.case`) is queued into that style's shaped set, so every glyph
`AtlasShaper` can return has a cell. `bake_atlas` gains `--layout` and
`--feature <tag>` (repeatable).

### 4.10 `AtlasShaper` and the shared trait

New module `src/font/atlas_shaper.rs`, no feature gate:

```rust
/// What a caller needs from a shaper, implemented by TextShaper (with the
/// `shaper` feature) and AtlasShaper.
pub trait RunShaper {
    fn shape(&self, text: &str) -> ShapedRun;
    fn units_per_em(&self) -> u16;
    /// Whether the face maps `ch` to a non-zero glyph.
    fn covers(&self, ch: char) -> bool;
}

pub struct AtlasShaper { face: FaceLayout }   // owned copy; small

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoLayout { NotBaked, StyleNotBaked(GlyphStyle) }

impl AtlasShaper {
    pub fn new(atlas: &FontAtlas, style: GlyphStyle) -> Result<Self, NoLayout>;
    pub fn style(&self) -> GlyphStyle;
    pub fn glyph_id_for_char(&self, ch: char) -> Option<u16>;
}
```

`AtlasShaper::shape` returns atlas addresses for its style, so a Regular
shaper's output equals `TextShaper::shape` and another style's equals
`StyledShaper::shape(style, ..)`. The algorithm, for left-to-right text:

1. For each `char` at byte offset `b`: glyph = `Cmap` lookup or 0;
   cluster = `b`; mark it ignorable if it is a default ignorable code point
   (the same predicate rustybuzz 0.20 uses, copied as a range table with its
   source cited; U+00AD is the one inside `TEXT_RANGES`).
2. For each `Gsub` lookup in order, scan left to right, skipping ignorable
   glyphs when matching:
   - kind 1: replace the glyph if it has a record;
   - kind 2: at each position, try the ligatures whose first component is
     this glyph in table order, first match wins; the match replaces the
     components with `out`, takes the smallest cluster of the span, gives
     that cluster to every ignorable glyph inside the span, and moves those
     ignorable glyphs after the ligature;
   - kind 3: replace the input glyph when the nearest non-ignorable glyph
     before it (side 0) or after it (side 1) is the context glyph, matching
     against the buffer as already rewritten by this lookup to the left.
3. Advance of each glyph = `Hmtx`; ignorable glyphs become `hidden_glyph`
   with advance 0 (or are removed when `hidden_glyph` is 0).
4. For each non-ignorable glyph, find the next non-ignorable glyph; add the
   `Gpos` value of the pair to the first one's advance.
5. Drop glyph 0 whose source character is a control character (as
   `TextShaper` does); x and y offsets are 0; `total_advance` is the sum.
6. Map raw ids to addresses with `style.styled_glyph_id` (0 stays 0).

**The contract.** For any string whose characters are in the face's `Cmap`
scope, or unmapped by the face, the output equals `TextShaper`'s with the
bake's features. For a character the face maps but the scope does not
include, `AtlasShaper` returns glyph 0 with the `.notdef` advance where
`TextShaper` returns the face's glyph; both draw the placeholder box (that
glyph has no cell), but the advance can differ. The docs say so.

`TextShaper` gets `impl RunShaper` (forwarding to its existing methods).
`push_shaped_text` does not change.

### 4.11 Dependencies and features

```toml
[dependencies]
rustybuzz = { version = "0.20", optional = true }
ttf-parser = { version = "0.25", optional = true }

[features]
default = ["shaper"]
shaper = ["dep:rustybuzz", "dep:ttf-parser"]
cpu-bake = ["shaper", "dep:msdfgen", "dep:ttf-parser-018"]
```

Behind `shaper`: `TextShaper`, `StyledShaper`, `NoFaceForStyle`,
`FontAtlasBuilder`, `glyph_projection`, `GlyphProjection`,
`default_cell_metrics`, `cap_height`, `extract_outline`, `FaceLayout::extract`,
`LayoutOptions`, `LayoutError`. Always available: `FontAtlas` (reading,
writing, layers, sets, `layout`, `licences`), `GlyphEntry`, `ShapedRun`,
`ShapedGlyph`, `AtlasShaper`, `RunShaper`, `AtlasLayout`, `FaceLayout`'s
data, `LicenceRecord`, the drawlist, the GPU renderer and `MsdfCompute`
(which takes a `GlyphOutline`; producing one from a face needs `shaper`).
`TextShaper::grapheme_indices`, `is_emoji` and `is_cjk` stay on `TextShaper`.

`cargo build --no-default-features` and
`cargo check --no-default-features --target wasm32-unknown-unknown` must
succeed with neither rustybuzz nor ttf-parser in `cargo tree`.

### 4.12 Versioning under Rule 5

**This release is 0.4.0, not 0.3.1.** It is not strictly additive under Rule
5.4 because `FontAtlas`, a struct whose fields are all public, gains two
fields (`layout`, `licences`): code outside libmsdf that builds a `FontAtlas`
with a struct literal stops compiling. No dependency changes version, so Rule
5.5 does not apply; making rustybuzz and ttf-parser optional behind a default
feature is additive.

**The file format.** Rule 5.4 is about the public API, and a file format is
data, not API. But a 0.4.0 bake writes files 0.3.x refuses, which a consumer
observes when it updates an artifact without updating libmsdf. Proposed rule
(to be stated in `VERSIONS.md` entries from this one on, without changing
AGENTS.md): every entry has a **Format** line naming the atlas format
version it writes and the versions it reads; a release that starts writing a
format older releases refuse is **not strictly additive** and bumps the minor
version even without an API change; a release that only adds reading of
something is additive. Here both reasons point to 0.4.0. Under this rule a
patch release of libmsdf never changes the format it writes, beyond the
format's own patch number.

`VERSIONS.md` for 0.4.0 lists: **Breaking:** `FontAtlas` gains `layout` and
`licences`; **Breaking (format):** writes atlas 5:0.1.0 (reads 3, 4 and
5:0.1); the default feature set gains `shaper` (a consumer with
`default-features = false` loses `TextShaper` and the builder until it
enables `shaper`); added: the version types and constants, `AtlasLayout`,
`FaceLayout`, `AtlasShaper`, `RunShaper`, `NoLayout`, `LicenceRecord`,
`LICENCE_REGISTRY`, `LayoutOptions`, `LayoutError`,
`FontAtlasBuilder::{with_layout, add_licence}`.

## 5. Steps

Each step is one commit on a branch `impl/0.4.0` touching only the libmsdf
crate (plus repo-level files in their own commits, Rule 3.1), passing
`cargo fmt -- --check`, `cargo clippy --all-targets` with no new warnings and
`cargo test`. Steps that touch baking also run
`cargo test --features cpu-bake` and the GPU suite
(`cargo test --features gpu-tests,cpu-bake`). `main` receives the steps as
the commits are made, or as one squash, as the 0.3.0 landing did.

1. **Version field and the v5 container.** Add `AtlasFormatVersion`,
   `AtlasFileVersion`, `ATLAS_FILE_MAJOR`, `ATLAS_FORMAT_VERSION`,
   `ATLAS_HEADER_SIZE_V5`, the gating helper, `FontAtlas::file_version`, the
   directory reader and writer with the rules of section 4.2 (unknown tags
   skipped, OpenType and malformed tags refused), and the two new
   `FontAtlas` fields (`layout: None`, `licences` empty, no table bodies
   yet). `to_bytes` writes 5:0.1.0; `from_bytes` reads 3, 4 and 5:0.1. Move
   the current fixture to `tests/fixtures/legacy/roboto-ascii-48-v3.atlas`
   (byte-identical, `git mv`).
   **Done when:** the test table of section 4.1 passes row by row; the legacy
   fixture loads and draws as before (the existing coverage and golden tests
   pass against it); a v5 round trip of it is identical in every field and
   pixel; a file with an unknown first-capital tag loads with that table
   skipped; files with tag `GSUB`, `cmap`, `fpgm` or `prep` in the directory,
   overlapping or misaligned bodies, or non-zero padding are refused with a
   message naming the rule.

2. **Table codecs.** `src/font/layout.rs` and `src/font/licence.rs`: the
   in-memory types of sections 4.7 and 4.8, their byte encodings, every
   limit and validation listed, `LICENCE_REGISTRY`, `FontAtlas::licences`.
   No font parsing.
   **Done when:** each table round-trips through bytes for hand-built
   examples (including an empty `Gsub` and `Gpos`); unsorted, duplicate,
   out-of-limit and wrong-length bodies are refused; a face with only some of
   the five layout tables is refused; every registry row round-trips and a
   registry fourcc with a contradicting SPDX string is refused; a record with
   an unknown fourcc loads and is exposed raw.

3. **Bake extraction and licences.** `src/font/layout_bake.rs`
   (`FaceLayout::extract` without the check yet), `FontAtlasBuilder::{with_layout,
   add_licence}`, closure cells queued into the shaped sets, licence
   validation against `name` IDs 0/13/14, and `bake_atlas`'s `--layout`,
   `--feature` and `--licence` flags. `build()` refuses a style without a
   licence.
   **Done when:** for the three bundled Roboto faces the extracted tables
   match the fontTools numbers of section 3 (Roboto Regular: 4 ligatures, the
   digit substitutions, the 434 non-zero pairs among text glyphs
   including (space, T) = -40, plus any the figure forms add,
   features kern liga lnum pnum); a bake without `--licence` fails naming the
   style; `--licence OFL-1.1=Roboto` on Roboto Bold fails because its `name`
   table says Apache-2.0; a lookup type the tables cannot carry, in a test
   face built for it, gives `UnsupportedLookup`.

4. **`AtlasShaper` and `RunShaper`.** `src/font/atlas_shaper.rs`, the
   default-ignorable range table, `impl RunShaper for TextShaper`.
   **Done when:** for the bundled faces, "Hello", "AV", "fifty", "office",
   "T ", " T", "f\u{AD}i", "T\u{AD} ", "0123", "a\tb" and "José" shape with
   `AtlasShaper` exactly as with `TextShaper` / `StyledShaper` (all fields);
   a legacy atlas gives `NoLayout::NotBaked`; a run from `AtlasShaper` drawn
   with `push_shaped_text` produces the same `DrawList` as the `TextShaper`
   run.

5. **The equivalence check.** Wire the corpus of section 4.9 into
   `FaceLayout::extract`, with `LAYOUT_CHECK_LEDGER` and the pangrams as
   constants, and the `Mismatch` error.
   **Done when:** extraction passes for Roboto Regular, Bold and Italic and
   for Regular+msymbols; a test that corrupts one `Gpos` value or reorders
   two `Gsub` lookups is caught as `Mismatch` naming the string; the check
   for one face finishes in under 10 s in a release build (measured and
   logged); run by hand with `bake_atlas --layout` on the tower defense
   game's Grenze Gotisch and Texturina files (read from their location, not
   copied), Texturina passes and Grenze passes or fails exactly as open
   question 1 decides, logged under `out/layout/` (Rule 3.5).

6. **Optional dependencies.** The `shaper` feature and the gates of section
   4.11.
   **Done when:** `cargo build --no-default-features` and the wasm32 check
   without default features pass; `cargo tree --no-default-features` lists
   neither rustybuzz nor ttf-parser; a test compiled without default features
   loads the step-7 fixture and shapes "fifty Tax" with `AtlasShaper`; the
   default build and `--features cpu-bake` are unchanged in behaviour.

7. **Fixtures, scripts and consumers.** Re-bake
   `tests/fixtures/roboto-ascii-48.atlas` as 5:0.1.0 with
   `--layout --licence Apache-2.0=Roboto`; update `scripts/bake-atlases.py`
   (its own commit) to pass the licence flags for both bundled bakes and
   `--layout`. Tests that load the fixture use the new file; the legacy one
   keeps the compatibility tests. Build the consumers as in the 0.3.0 landing
   (scratch copies, nothing committed in them).
   **Done when:** `bake-atlases.py --check` passes against the new fixture;
   the new fixture's pixel data and glyph entries are byte-identical to the
   legacy fixture's (only header, layer table, directory and tables differ);
   libcontrols, tinytrowel, sigilweaver and the tower defense game build and
   their tests pass against the new tree with default features, and the
   game's two committed v3 atlases still load.

8. **Release 0.4.0.** Version bump, `VERSIONS.md` entry of section 4.12 with
   its Format line, tag `v0.4.0` (Rule 5.1), README row updated when the
   status changes.
   **Done when:** the tag exists on the commit that changes the version, and
   the entry lists every public change named in section 4.12.

## 6. Open questions

1. **Grenze's contextual accents.** Accept `Gsub` kind 3 (one-input,
   one-context-glyph single substitution, section 4.6), or keep decision 5
   strictly, in which case Grenze fails the bake and the game keeps shaping
   it with rustybuzz? Recommended: accept; it is a small, flattened record
   with no general context machinery, and it covers the only live contextual
   rule found. A third option, leaving the four spacing accents out of
   Grenze's `Cmap` scope, would draw them as boxes and is not recommended.
2. **Kerning units.** Confirm integer font units with `Head.units_per_em`
   (section 4.6) instead of em values, for exact equality with rustybuzz.
3. **Optional features: per atlas, per face or per set?** This plan records
   them per face (`Head.features`), chosen at bake for all faces alike via
   `--feature`. Should a style be able to take different optional features
   (say `tnum` in the regular face only), or should an atlas carry two
   variants of a face?
4. **Vertical metrics.** `SetMetrics` already carries ascent, descent, line
   gap and underline per set. Should `Head` also carry them per face, so a
   layout-only consumer never needs the set table, or is the set table the
   one home?
5. **Fixture size.** Step 7 keeps the 2.4 MB legacy fixture beside the new
   one so legacy reading is tested against real historical bytes. Accept the
   repository growth, or synthesise legacy bytes from the new fixture in the
   tests instead?
6. **Semver past major 0.** From 5:1.0.0 on, should a reader accept a file
   with a higher minor in its own major (minor meaning "only adds tables an
   older reader can skip"), or keep refusing every newer version as at
   major 0?
7. **Format line in `VERSIONS.md`.** Accept the rule of section 4.12 (a
   release that starts writing a format older releases refuse bumps the
   minor version), or put it into AGENTS.md Rule 5 for all repositories that
   share it?

## Amendment — 2026-10-16 12:41 EDT: `LICT`, the text companion to `LICS`

Owner's decision, relayed: "LICT is n-count and ascii of the copyright/license
name (like a c++ symbol)". The licence strings move out of `LICS` into a new
all-capital table, `LICT`, so `LICS` becomes fixed-size.

**What this replaces.** In section 4.8, the `LICS` byte layout (the block
from "0   2  count u16" to "zero padding to a multiple of 4") and the
sentence "Strings are at most 4096 bytes; records are in bake order; at least
one record per style the atlas carries cells for." are replaced by the two
layouts below. Section 4.3's list of all-capital tags becomes `LICS` and
`LICT`. Everything else in section 4.8 stands: the registry, the bake rules,
the reader's handling of unknown fourccs, and `LicenceRecord`, whose public
fields are unchanged and whose strings are now read from `LICT`. Step 2's
**Done when:** gains the `LICT` tests listed at the end of this amendment.

**`LICS`** (one per atlas, `face` = 0xFF), 4 + 12 × count bytes:

```text
0   2  count u16 (at least 1 in a baked atlas)
2   2  reserved 0
then count records, 12 bytes each, in bake order:
0   4  licence fourcc   [u8; 4], registry of section 4.8
4   4  licence version  [major, minor, patch, subpatch]
8   1  styles           bitmask of GlyphStyle ordinals covered
9   1  reserved 0
10  2  lict_index       u16: index of this record's entry in LICT
```

At least one record covers each style the atlas has cells for.

**`LICT`** (one per atlas, `face` = 0xFF), required whenever `LICS` is
present and absent otherwise:

```text
0   2  count u16, equal to LICS's count
2   2  reserved 0
4      count entries, back to back, in the same order as LICS's records.
       Each entry is four strings in this fixed order:
         spdx, source, copyright, rfn
       and each string is written as its length in decimal ASCII digits
       followed immediately by that many bytes of text, with no separator,
       the way Itanium C++ mangled names encode identifiers.
then zero padding to a multiple of 4 (outside the table's length)
```

Example entry for Grenze Gotisch, whose `name` ID 0 is 98 characters long
and which has no Reserved Font Name:

```text
7OFL-1.114Grenze Gotisch98Copyright 2020 The Grenze Gotisch Project Authors (https://github.com/Omnibus-Type/Grenze-Gotisch)0
```

**Reference between the two.** LICS record `i` names its entry with
`lict_index`, and entries are written in LICS order, so a valid file always
has `lict_index == i`. A reader refuses a file where any `lict_index` differs
from its position, where the two counts differ, or where `LICT` has bytes
left over after the last entry. The index is stored explicitly anyway so a
later minor version can let records share an entry (for example two faces
with the same licence and copyright) without changing `LICS`'s record size;
that would be gated on the version, as section 4.1 describes.

**Encoding rules.** Each is checked by the encoder at bake and by the decoder
on read:

1. **Counts.** Decimal ASCII digits `0`-`9`, no sign, no leading zeros: a
   count that starts with `0` is exactly `0` (the empty string). A count is at
   most 4096, so at most four digits. A count over 4096, or a fifth digit, is
   refused.
2. **Text.** Printable ASCII only, bytes 0x20 to 0x7E. **A string that is not
   ASCII is refused at bake**, naming the face, the field and the first
   non-ASCII character; the bake does not escape it. Escaping (for example
   `\u{...}` sequences or Punycode) was considered and is not recommended: it
   would make the stored text differ from the face's own copyright line,
   which is the text a licence like OFL asks to be carried, and every face
   measured for this plan (section 3.5) is plain ASCII in every field the
   bake reads.
3. **No leading digit.** Because a count runs into its text with no
   separator, a string whose first character is a digit cannot be told apart
   from the end of its count (`"3D"` would be written `23D`, which reads back
   as a count of 23). C++ identifiers avoid this by never starting with a
   digit, and so do these strings: a non-empty string whose first byte is
   `0`-`9` is refused at bake with an error naming the field. None of the
   measured values starts with a digit (copyright lines start with
   "Copyright", SPDX identifiers with a letter).
4. **Truncation.** A count that runs past the end of the table, digits with no
   text after them, or a table that ends in the middle of an entry, is
   refused.

The codec is a pair of functions in `src/font/licence.rs`:

```rust
/// Append `s` as <decimal length><bytes>. Errors: not printable ASCII,
/// starts with a digit, longer than 4096.
fn put_counted(out: &mut Vec<u8>, s: &str) -> Result<(), LicenceTextError>;
/// Read one counted string at `*pos`, advancing it. Errors: no digits,
/// leading zero, more than four digits or over 4096, truncated, non-ASCII,
/// leading digit in the text.
fn take_counted<'a>(data: &'a [u8], pos: &mut usize) -> Result<&'a str, LicenceTextError>;
```

**Tests added to step 2's Done when:**

- `7OFL-1.1` reads as `OFL-1.1`, leaving the position after it;
- counts of two, three and four digits read correctly (a 52-byte copyright
  line, a 98-byte one, a 4096-byte string), and a whole Grenze-style entry
  reads back as its four strings;
- `0` reads as the empty string and the next count follows immediately
  (`0` then `7OFL-1.1` in one buffer gives `""` then `OFL-1.1`);
- refused: a leading zero (`07OFL-1.1`), a count over 4096 (`4097...`), a
  fifth digit (`10000...`), a missing count (text where digits should be),
  truncated text (`10abc` at the end of the table), digits at the very end
  with no text, a non-ASCII byte in the text, and a text starting with a
  digit;
- the encoder refuses non-ASCII text, a leading digit and an over-long
  string, and its output round-trips through the decoder for every licence
  string of the bundled faces;
- a `LICS` with a `lict_index` that differs from its position, or with a
  count that differs from `LICT`'s, is refused; a `LICS` without a `LICT` is
  refused.

## Amendment — 2026-10-16 12:56 EDT: `LICT` strings are length-byte prefixed

Owner's correction, relayed: "I stated that wrong, it should be a byte length
followed by ascii". Each `LICT` string is one binary length byte followed by
that many printable ASCII bytes (a Pascal-style string), not a decimal count.

**What this replaces.** In the 12:41 EDT amendment:

- in the `LICT` layout, the text "each string is written as its length in
  decimal ASCII digits followed immediately by that many bytes of text, with
  no separator, the way Itanium C++ mangled names encode identifiers" is
  replaced by "each string is written as one length byte (u8, 0 to 255)
  followed by that many bytes of text";
- the worked Grenze example is replaced by the one below;
- the **Encoding rules** (1 to 4) are replaced by the rules below. Rule 1
  (decimal counts, no leading zeros, at most four digits, at most 4096) and
  rule 3 (no string may start with a digit) are withdrawn, since a binary
  length byte cannot run into its text;
- the codec signatures and step 2's added tests are replaced by the ones
  below.

Everything else in the 12:41 amendment stands: the fixed-size `LICS` record,
`LICT`'s header and entry order (spdx, source, copyright, rfn), the
`lict_index` reference and its checks.

**Worked example** (Grenze Gotisch, no Reserved Font Name), as bytes:

```text
0x07 "OFL-1.1"
0x0E "Grenze Gotisch"
0x62 "Copyright 2020 The Grenze Gotisch Project Authors (https://github.com/Omnibus-Type/Grenze-Gotisch)"
0x00
```

(0x62 = 98, the length of that `name` ID 0 line.)

**Encoding rules**, checked by the encoder at bake and by the decoder on
read:

1. **Length.** One byte, 0 to 255. 0 is the empty string. **A string longer
   than 255 bytes is refused at bake**, naming the face and the field. Every
   string measured for this plan fits: the longest is Grenze Gotisch's
   98-byte copyright line (section 3.5); Texturina's is 88 bytes and Roboto's
   47.
2. **Text.** Printable ASCII only, bytes 0x20 to 0x7E. A string that is not
   ASCII is refused at bake, not escaped, for the reason the 12:41 amendment
   gives.
3. **Truncation.** A length byte that runs past the end of the table, or a
   table that ends in the middle of an entry (including right after a length
   byte), is refused.
4. **Structure** (unchanged from 12:41): `LICT`'s count equals `LICS`'s;
   every `lict_index` equals its record's position; no bytes are left over
   after the last entry; a `LICS` without a `LICT` is refused.

**Codec**, in `src/font/licence.rs`:

```rust
/// Append `s` as <u8 length><bytes>. Errors: not printable ASCII, longer
/// than 255 bytes.
fn put_pstr(out: &mut Vec<u8>, s: &str) -> Result<(), LicenceTextError>;
/// Read one length-prefixed string at `*pos`, advancing it. Errors: no
/// length byte, length past the end, non-ASCII byte.
fn take_pstr<'a>(data: &'a [u8], pos: &mut usize) -> Result<&'a str, LicenceTextError>;
```

**Tests added to step 2's Done when** (replacing the 12:41 list):

- `0x07 "OFL-1.1"` reads as `OFL-1.1`, leaving the position after it;
- length 0 reads as the empty string and the next string follows
  immediately (`0x00` then `0x07 "OFL-1.1"` gives `""` then `OFL-1.1`);
- length 255 with 255 bytes of text reads back whole;
- the whole Grenze example entry above reads back as its four strings;
- refused: a length past the end of the table (`0x0A "abc"` at the end),
  a length byte with nothing after it, a table ending before an entry's
  fourth string, and a non-ASCII byte (0x80 and above, or a control byte
  below 0x20) in the text;
- the encoder refuses non-ASCII text and a 256-byte string, and its output
  round-trips through the decoder for every licence string of the bundled
  faces;
- a `LICS` whose `lict_index` differs from its position, or whose count
  differs from `LICT`'s, is refused; a `LICS` without a `LICT` is refused.

## Amendment — 2026-10-16 13:02 EDT: open questions decided; file-major comes first

The owner asked the coordinating agent to decide the seven open questions of
section 6 ("decide those"). The decisions below are relayed from it and
replace section 6: **section 6 has no open questions left.** Each decision
states which earlier text it replaces or settles. This amendment also records
a question the owner asked about the version field, with its answer.

### Owner's question: "does file-major need to be last for back compat?"

**No. It must be first,** as section 4.1 already has it (byte 4 = file-major,
then major, minor, patch). The reasons:

- Old files store the version as a little-endian u32 at bytes 4..8, so v3 is
  `03 00 00 00` and v4 is `04 00 00 00`. Read as
  `[file_major, major, minor, patch]`, they land exactly on file-major 3 or 4
  with version 0.0.0, which is what lets one reader tell legacy from split
  without guessing.
- Old readers read a new file's `05 00 01 00` as the u32 65541
  (5 + 1 × 65536). That is neither 3 nor 4, so every libmsdf before this one
  refuses it cleanly with its existing "atlas version is not one this build
  reads" error, instead of misparsing it.
- If file-major were last, a v3 file (`03 00 00 00`) would read as major 3,
  minor 0, patch 0 of file-major 0: a legacy file would be taken for a
  split version, the opposite of what the field is for.

Section 4.1's text stands unchanged; this records why.

### Decisions

1. **Grenze's contextual accents: `Gsub` kind 3 is accepted.** The narrow
   "context single" kind (one input glyph, one context glyph on one side,
   single substitution, flattened over the glyph classes) is part of the
   format, proven by the equivalence check like every other kind, so Grenze
   Gotisch bakes. This settles section 4.6's "proposed, pending the owner's
   answer to open question 1", section 4.7's comment "only if open question 1
   is accepted" on `GsubRules::ContextSingle`, section 4.9's "if accepted"
   and the expected result there (Grenze passes), and step 5's "Grenze passes
   or fails exactly as open question 1 decides" (Grenze passes). Any other
   contextual shape still fails the bake.
2. **Kerning is in integer font units**, with `Head.units_per_em`, as section
   4.6 proposes. This settles that refinement of decision 2 of section 2.
3. **Optional features are chosen per face** and recorded in that face's
   `Head.features`, as section 4.5 already lays out. This replaces section
   4.9's "chosen at bake for all faces alike via `--feature`":
   `bake_atlas --feature <tag>[@<style>,...]` (repeatable) applies the
   feature to the listed styles, defaulting to `regular`, and
   `LayoutOptions.extra_features` is applied to the face it is passed with.
   An atlas does not carry two variants of one face.
4. **`Head` carries vertical metrics.** This replaces section 4.5's `Head`
   layout. The new layout, 20 + 4 × feature_count bytes, all values in font
   units:

   ```text
   0   2  units_per_em    u16 (16..=16384)
   2   2  hidden_glyph    u16 (as before)
   4   2  ascender        i16, hhea ascender (positive above the baseline)
   6   2  descender       i16, hhea descender (negative below it)
   8   2  line_gap        i16, hhea lineGap
   10  2  cap_height      i16: OS/2 sCapHeight when the OS/2 table is version
                          2 or later and the value is non-zero; otherwise the
                          yMax of the bounding box of 'H'; 0 if neither exists
   12  2  x_height        i16: OS/2 sxHeight under the same rule; otherwise the
                          yMax of the bounding box of 'x'; 0 if neither exists
   14  2  feature_count   u16
   16  4  reserved        0
   20     feature tags    [u8; 4] x feature_count, ascending, unique
   ```

   `FaceLayout` gains `ascender`, `descender`, `line_gap`, `cap_height` and
   `x_height` (all `i16`). They are per face and in font units, so a
   layout-only consumer can set lines without the set table; `SetMetrics`
   stays as it is (per set, in em, with the measured ink descent). Step 3's
   **Done when:** gains: for the three Roboto faces the five values equal
   fontTools' readings of `hhea` and `OS/2` (or the stated fallback).
5. **Keep the real legacy fixture.** The existing
   `tests/fixtures/roboto-ascii-48.atlas` (a real v3 file, already in
   history) moves to `tests/fixtures/legacy/roboto-ascii-48-v3.atlas` as
   step 1 says; tests do not synthesise legacy bytes. This settles step 7's
   fixture arrangement as written.
6. **Semver past major 0.** While major is 0 a reader refuses a newer minor,
   as section 4.1 says. From major 1 a reader **accepts** a newer minor within
   its own major, and minor releases from 1.0 may then only add tables that an
   older reader skips (section 4.2's rule for unknown tags), never change the
   layout of the fixed prefix or of an existing table; a newer major is
   refused. Patch never affects reading. This replaces section 4.1's sentence
   "Semver past major 0 (what a minor bump may and may not do from 1.0) is
   left open (open question 6); this plan writes only 0.1.0." (the plan still
   writes only 0.1.0), and extends its acceptance rule: for `F.major >= 1`
   the refusal condition is `F.major > K.major`, not
   `(F.major, F.minor) > K`. Step 1's test table gains two rows, run against
   a hypothetical 5:1.0 reader through the gating helper: `05 01 02 00` is
   accepted (unknown tables skipped) and `05 02 00 00` is refused.
7. **The Format line becomes part of Rule 5.** libmsdf's Rule 5 gains the
   rule of section 4.12: every `VERSIONS.md` entry has a **Format** line
   naming the atlas format version it writes and the versions it reads, and
   a release that starts writing a format older releases cannot read bumps
   the minor version even without a public API change. This is a separate
   change to AGENTS.md that this plan proposes; **it is not written now**,
   and is made as its own commit, with its own CHANGES.md addendum, when the
   owner approves the wording. Until then section 4.12 applies to the 0.4.0
   entry as written there.

## Amendment — 2026-10-16 13:03 EDT: the format version follows semver as Cargo reads it

The owner confirmed, relayed: "right, it follows semver post 1.0, moves
left". The format version within a file-major reads the way Cargo reads
semver:

- **From 1.0:** major = breaking (a reader refuses a newer major), minor =
  additive (a reader accepts a newer minor in its major), patch = no format
  change.
- **Before 1.0 the meanings move one place left:** minor = breaking (a
  0.x minor bump may change parsing; a reader refuses a newer minor), patch
  = additive (a reader accepts a newer patch within its minor).
- **0.0.x** files remain refused.
- **Additive** means only adding tables that an older reader skips
  (section 4.2's rule for unknown tags). It never changes the fixed prefix,
  the layout of an existing table, or how any existing data is read.

**What this replaces.**

- Section 4.1 and decision 1 of section 2 said "Patch never changes parsing"
  and "Patch is ignored: two files that differ only in patch parse
  identically. A patch bump records a writer change that leaves the layout
  the same". These now apply only from 1.0. In 0.x a patch may add skippable
  tables, and a reader that does not know them skips them; it still never
  changes how existing data is read.
- Decision 6 of the 13:02 EDT amendment is refined. Its 1.0 rules stand,
  with patch at 1.x meaning no format change. Its 0.x rule ("a reader
  refuses a newer minor") stands, and a newer patch within the reader's
  minor is accepted.
- Section 4.1's acceptance rule for `F.major == 0` stays as written:
  refuse when `(F.major, F.minor) > K` or `(F.major, F.minor) < (0, 1)`,
  otherwise accept whatever the patch. For `F.major >= 1` it stays as
  decision 6 set it: refuse when `F.major > K.major`.

**Step 1's test table** (section 4.1, with the 13:02 additions) is replaced
by the rows below for the split versions. The legacy rows (`03`, `04`,
`03 00 01 00`, `02`, `00`) are unchanged.

| Bytes 4..8 | Reader newest | Result | Sections read |
|---|---|---|---|
| `05 00 01 00` | 5:0.1.0 | accept | 36-byte header, layers, sets, glyphs, directory, tables, pixels |
| `05 00 01 01`, with one table of a tag the reader does not know | 5:0.1.0 | accept; the unknown table is skipped (bounds checked, body not read) | as 0.1.0 |
| `05 00 01 07` | 5:0.1.0 | accept, parsed as 0.1.0 plus any skipped tables | as 0.1.0 |
| `05 00 00 03` | 5:0.1.0 | refuse (0.0.x never written) | none |
| `05 00 02 00` | 5:0.1.0 | refuse (newer minor in 0.x) | none |
| `05 01 00 00` | 5:0.1.0 | refuse (newer major) | none |
| `06 00 00 00` | 5:0.1.0 | refuse (newer file-major) | none |
| `05 00 01 00` | hypothetical 5:0.2 reader, field X gated `v > (0, 1)` | accept | as 0.1, X takes its default |
| `05 00 02 00` | hypothetical 5:0.2 reader | accept | as 0.1, plus X |
| `05 01 02 00` | hypothetical 5:1.0 reader | accept (newer minor from 1.0); unknown tables skipped | as 1.0 |
| `05 01 00 05` | hypothetical 5:1.0 reader | accept, parsed as 1.0.0 | as 1.0 |
| `05 02 00 00` | hypothetical 5:1.0 reader | refuse (newer major) | none |

The hypothetical-reader rows remain unit tests of the gating and acceptance
helpers, not of a real 0.2 or 1.0 format.

## Amendment — 2026-10-16 13:17 EDT: implementation started; the document is immutable

The owner approved this plan on 2026-10-16 ("approve and build"), with every
amendment above in force (the 12:41 and 12:56 EDT `LICT` amendments, the
13:02 EDT decisions and the 13:03 EDT semver reading). Implementation starts
with step 1 on the branch `impl/0.4.0`, and the status is now
`In implementation`. From here the document is immutable (Rule 1.5):
departures are recorded in `CHANGES.md`, and only the owner's explicit
instruction changes it.
