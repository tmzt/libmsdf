//! Build-time atlas bake helper (native-only): font file → baked MSDF
//! atlas bytes (`FontAtlas::to_bytes` format, loadable on wasm32 where the
//! CPU msdfgen path doesn't exist).
//!
//! **Prefer `scripts/bake-atlases.py`**, which runs this for all three
//! committed artifacts with the flags each one needs and reports the cell
//! delta. Reach for the raw invocation below only for a one-off bake that is
//! not a committed artifact - the flags are NOT interchangeable between the
//! three, and getting them wrong produces a file that loads and draws.
//!
//! Usage:
//! ```text
//! cargo run -p libmsdf --example bake_atlas -- <font.ttf> <out.atlas> [glyph_size] [px_range]
//!                                             [--style <bold|italic|bolditalic>[=<face.ttf>]]...
//! ```
//! With no arguments, bakes the bundled Roboto ASCII subset at 48px/6.0
//! to `roboto-ascii-48.atlas` in the current directory — the shipped
//! `highbay/src/assets/roboto-ascii-48.atlas`. `px_range` must be large enough
//! that the smallest style still gets a pixel of distance range: see
//! [`libmsdf::FontAtlas::min_antialiased_font_size`] (48px cells at 6.0 →
//! 6.15px, covering the ZUI's 8px zoomed-out graph labels).
//!
//! What gets baked is [`libmsdf::FontAtlasBuilder::add_shipped_coverage`] and
//! nothing else — the text ranges, whatever the face defines in the **Private
//! Use Area** (which is where an icon font puts its glyphs), and glyph 0's
//! placeholder box. This tool therefore never learns an icon set or a
//! codepoint list; changing coverage is a change to that one method. That is
//! how `libhbui`'s atlas gets its Material Symbols cells:
//!
//! ```text
//! cargo run -p libmsdf --features cpu-bake --example bake_atlas -- \
//!     deps/libmsdf/fonts/Roboto-Regular-ascii-msymbols.ttf \
//!     crates/libhbui/assets/roboto-msymbols-48.atlas 48 6.0 \
//!     --style bold --style italic
//! ```
//!
//! **THE TWO `--style` FLAGS ARE PART OF THAT COMMAND, and this paragraph
//! carried it without them for as long as layers have existed.** The committed
//! `libhbui` atlas is LAYERED - `(48, Regular)`, `(48, Bold)`, `(48, Italic)`,
//! which is what `libhbui::surface::BAKED_STYLES` declares and
//! `crates/libhbui/tests/emphasis.rs` holds it to. Running the command without
//! them writes a perfectly valid single-layer v3 file 430 cells smaller, and
//! every `**bold**` and `*italic*` run in the repo silently loses its cut. That
//! is the same shape of defect as a stale atlas: the artifact still parses,
//! still renders, and is wrong in a way no loader complains about. It is why
//! `scripts/bake-atlases.py` exists and why it, not this comment, is the
//! producer.
//!
//! Both bundled faces carry the blocks this repo DRAWS
//! ([`libmsdf::OWNED_BLOCKS`] — the edge markers and our own UI icons, as
//! opposed to the Material Symbols it borrows), so the two bakes differ only
//! by the borrowed cells - eighteen of them today, and they are baked LAST, so
//! the two atlases agree cell-for-cell everywhere else.
//!
//! Cells are laid out in the order [`libmsdf::GlyphSet`] declares -
//! `(set, glyph id)`, not the order the queue happened to be filled in - so a
//! re-bake after a coverage widening is a strict SUPERSET of the previous one
//! whenever the addition lands in the last set that has anything in it: every
//! earlier cell keeps its atlas coordinates, and the texture is pinned so it
//! does not even get taller. A rendered frame that moves after a re-bake is a
//! real finding.
//!
//! (The glyph-TABLE index is a different thing and is not promised to be
//! stable: the table is sorted by glyph id, so a new glyph takes its place
//! among the others. Nothing outside a loaded atlas can see that - the GPU
//! table and every index packed into a draw list come from the same instance.)
//!
//! # `--style`, and why it does not fit yet
//!
//! `--style bold` queues [`libmsdf::ROBOTO_BOLD_ASCII`] as
//! [`libmsdf::GlyphStyle::Bold`]; `--style bold=<face.ttf>` queues a face of
//! your own. Styled sets are declared after every unstyled one, so this is a
//! pure append: every cell the shipped coverage lays down keeps its
//! coordinates, and a frame that moves after a styled re-bake is a real
//! finding.
//!
//! # `--style` fits now, because a style is a LAYER
//!
//! It did not when this flag was written: the shipped coverage is 239 cells of
//! a 320-cell grid, a style is 215 (201 declared codepoints plus 14 shaped
//! forms), and the grid was the whole atlas's budget. It is now one
//! [`libmsdf::AtlasLayer`]'s budget - a layer is `(point size, style)` - so
//! `--style bold --style italic` bakes 669 cells into three layers of 239, 215
//! and 215, and the texture is the same 400x2000 it was.
//!
//! Two consequences for anyone running this:
//!
//! * **A styled atlas is written at [`libmsdf::ATLAS_VERSION_LAYERED`]**, and a
//!   build that reads only [`libmsdf::ATLAS_VERSION`] refuses it by name. An
//!   UNSTYLED bake is still v3, byte for byte what it was, which is why the
//!   three committed atlases did not have to be re-baked for any of this.
//! * **The consumer has to create its texture with the layer count.** wgpu
//!   fixes an array's layers at creation, so
//!   `GpuSdfRenderer::new_with_msdf_layers(.., atlas.layer_count())` is
//!   required before a layered atlas will draw; `upload_msdf_atlas` uploads
//!   every layer it is given and logs an error if the texture has fewer.
//!
//! MEASURED on the merged 48px face, gzip -9 (2026-08-29, after the
//! typographic ten): the base atlas is 307,088 bytes, `--style bold` is 625,105
//! (+318,017) and `--style bold --style italic` is 985,000 (+359,895 for
//! italic). Raw size is not the measure here - the
//! texture is pinned, so a layer adds its whole 400x2000 rectangle whether or
//! not the cells fill it, and most of that is zeros.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let check = args.iter().any(|a| a == "--check");

    // `--style bold`, or `--style bold=<face.ttf>`. Parsed before the
    // positionals so a style argument can never be taken for the font or the
    // output path.
    let styles: Vec<(libmsdf::GlyphStyle, Vec<u8>)> = args
        .iter()
        .zip(args.iter().skip(1))
        .filter(|(flag, _)| *flag == "--style")
        .map(|(_, spec)| {
            let (name, path) = spec
                .split_once('=')
                .map_or((spec.as_str(), None), |(n, p)| (n, Some(p)));
            let style = match name.to_ascii_lowercase().as_str() {
                "bold" => libmsdf::GlyphStyle::Bold,
                "italic" => libmsdf::GlyphStyle::Italic,
                "bolditalic" => libmsdf::GlyphStyle::BoldItalic,
                other => panic!("--style takes bold, italic or bolditalic, not {other:?}"),
            };
            let data = match path {
                Some(p) => std::fs::read(p).unwrap_or_else(|e| panic!("read style face {p}: {e}")),
                // The bundled face, which is `None` for a style this build
                // ships nothing for - a missing asset the caller must supply,
                // never a substitution.
                None => libmsdf::bundled_style_face(style)
                    .unwrap_or_else(|| {
                        panic!("no face is bundled for {style:?} - pass --style {name}=<face.ttf>")
                    })
                    .to_vec(),
            };
            (style, data)
        })
        .collect();
    // `--licence <SPDX>=<source>[@<style>,...]` (repeatable; style defaults
    // to regular), `--feature <tag>[@<style>,...]` (repeatable; style
    // defaults to regular) and `--layout`.
    let parse_styles = |list: Option<&str>| -> Vec<libmsdf::GlyphStyle> {
        match list {
            None => vec![libmsdf::GlyphStyle::Regular],
            Some(l) => l
                .split(',')
                .map(|n| match n.to_ascii_lowercase().as_str() {
                    "regular" => libmsdf::GlyphStyle::Regular,
                    "bold" => libmsdf::GlyphStyle::Bold,
                    "italic" => libmsdf::GlyphStyle::Italic,
                    "bolditalic" => libmsdf::GlyphStyle::BoldItalic,
                    other => panic!("unknown style {other:?}"),
                })
                .collect(),
        }
    };
    let flag_values = |flag: &str| -> Vec<String> {
        args.iter()
            .zip(args.iter().skip(1))
            .filter(|(f, _)| *f == flag)
            .map(|(_, v)| v.clone())
            .collect()
    };
    let licences: Vec<(Vec<libmsdf::GlyphStyle>, String, String)> = flag_values("--licence")
        .iter()
        .map(|spec| {
            let (body, styles) = spec
                .split_once('@')
                .map_or((spec.as_str(), None), |(b, s)| (b, Some(s)));
            let (spdx, source) = body.split_once('=').unwrap_or_else(|| {
                panic!("--licence takes <SPDX>=<source>[@<style>,...], not {spec:?}")
            });
            (parse_styles(styles), spdx.to_string(), source.to_string())
        })
        .collect();
    let mut features: Vec<(libmsdf::GlyphStyle, [u8; 4])> = Vec::new();
    for spec in flag_values("--feature") {
        let (tag, styles) = spec
            .split_once('@')
            .map_or((spec.as_str(), None), |(t, s)| (t, Some(s)));
        let tag: [u8; 4] = format!("{tag:<4}")
            .as_bytes()
            .try_into()
            .unwrap_or_else(|_| panic!("--feature takes a 4-letter tag, not {tag:?}"));
        for style in parse_styles(styles) {
            features.push((style, tag));
        }
    }
    let layout = args.iter().any(|a| a == "--layout");
    if !features.is_empty() && !layout {
        panic!("--feature needs --layout");
    }

    let args: Vec<String> = {
        let mut positional = Vec::new();
        let mut skip = false;
        for a in &args {
            if skip {
                skip = false;
                continue;
            }
            if a == "--style" || a == "--licence" || a == "--feature" {
                skip = true;
                continue;
            }
            if a.starts_with("--") {
                continue;
            }
            positional.push(a.clone());
        }
        positional
    };

    let (font_data, out_path) = if args.len() >= 3 {
        let data = std::fs::read(&args[1]).unwrap_or_else(|e| panic!("read font {}: {e}", args[1]));
        (data, args[2].clone())
    } else {
        (
            libmsdf::ROBOTO_REGULAR_ASCII.to_vec(),
            "roboto-ascii-48.atlas".to_string(),
        )
    };
    let glyph_size: u32 = args
        .get(3)
        .map(|s| s.parse().expect("glyph_size"))
        .unwrap_or(48);
    let px_range: f64 = args
        .get(4)
        .map(|s| s.parse().expect("px_range"))
        .unwrap_or(6.0);

    let mut builder = libmsdf::FontAtlasBuilder::new(font_data, glyph_size, px_range);
    builder.add_shipped_coverage();
    for (style, data) in styles {
        builder
            .add_styled_coverage(style, data)
            .unwrap_or_else(|e| panic!("{e}"));
    }
    for (styles, spdx, source) in &licences {
        builder
            .add_licence(styles, spdx, source)
            .unwrap_or_else(|e| panic!("--licence: {e}"));
    }
    if layout {
        let started = std::time::Instant::now();
        for style in builder.styles() {
            let extra_features = features
                .iter()
                .filter(|(s, _)| *s == style)
                .map(|(_, t)| *t)
                .collect();
            builder.with_style_layout(style, libmsdf::LayoutOptions { extra_features });
        }
        eprintln!(
            "bake_atlas: layout tables requested for {:?}",
            builder.styles()
        );
        let atlas = builder.build().unwrap_or_else(|e| {
            eprintln!("bake_atlas: FAILED - {e}");
            std::process::exit(1);
        });
        eprintln!(
            "bake_atlas: layout extracted and checked in {:.2?} (whole bake)",
            started.elapsed()
        );
        // The check once more per face, timed on its own, for the log: the
        // corpus size and how long one face's check takes.
        if let Some(layout) = &atlas.layout {
            for face in layout.faces() {
                let data = builder
                    .face_data(face.style)
                    .expect("a baked style has a face");
                let opts = libmsdf::LayoutOptions {
                    extra_features: features
                        .iter()
                        .filter(|(s, _)| *s == face.style)
                        .map(|(_, t)| *t)
                        .collect(),
                };
                let t = std::time::Instant::now();
                let n = libmsdf::font::layout_bake::check_equivalence(data, face, &opts)
                    .expect("the bake's own check passed");
                eprintln!(
                    "bake_atlas: {:?}: {n} corpus strings shape identically with rustybuzz and \
                     AtlasShaper ({:.2?}); features {:?}; {} Gsub lookups, {} Gpos pairs",
                    face.style,
                    t.elapsed(),
                    face.features
                        .iter()
                        .map(|f| String::from_utf8_lossy(f).into_owned())
                        .collect::<Vec<_>>(),
                    face.gsub.len(),
                    face.gpos.len()
                );
            }
        }
        return finish(atlas, check, &out_path);
    }
    let atlas = builder.build().unwrap_or_else(|e| {
        eprintln!("bake_atlas: FAILED - {e}");
        std::process::exit(1);
    });
    finish(atlas, check, &out_path);
}

#[cfg(not(target_arch = "wasm32"))]
fn finish(atlas: libmsdf::FontAtlas, check: bool, out_path: &str) {
    let out_path = out_path.to_string();
    let bytes = atlas.to_bytes();

    // **`--check` compares instead of writing.** The atlas is a COMMITTED
    // artifact that five test files `include_bytes!`, produced by this example
    // run BY HAND. Nothing noticed when a face was re-baked and this was not
    // re-run: the stale bytes still parse, still render, and draw the old
    // glyphs. That is the same silent shape the `.hbdef` artifacts had before
    // `hb-pack --mode compile` gave them a producer, and this is the same
    // remedy.
    //
    // A byte comparison is sound because the bake is REPRODUCIBLE - verified
    // by baking twice and `cmp`-ing, 2406768 bytes identical. What is NOT
    // established is reproducibility ACROSS ARCHITECTURES: MSDF generation is
    // float math, and an arm64 and an x86 host have not been compared. Run
    // this check where the artifact was baked; if it ever runs somewhere else
    // and fails on pixels alone, compare the index (glyph ids, cell
    // coordinates, dimensions) rather than raising the tolerance.
    if check {
        let committed = std::fs::read(&out_path)
            .unwrap_or_else(|e| panic!("read {out_path} to check against: {e}"));
        if committed == bytes {
            println!("bake_atlas: OK - {out_path} is what this bake produces");
            return;
        }
        eprintln!(
            "bake_atlas: STALE - {out_path} is {} bytes and this bake produces {}.\n\
             \n\
             The committed atlas is not what the shipped face bakes to. Either the\n\
             face changed and this example was not re-run, or the atlas was hand-\n\
             edited. Re-run without --check to regenerate, and say WHICH of the two\n\
             it was - a regenerated artifact committed without that answer hides the\n\
             defect it was meant to surface.",
            committed.len(),
            bytes.len()
        );
        std::process::exit(1);
    }

    std::fs::write(&out_path, &bytes).expect("write atlas");
    println!(
        "baked {}: {}x{} px, {} glyphs, {} bytes",
        out_path,
        atlas.width,
        atlas.height,
        atlas.glyphs.len(),
        bytes.len()
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("bake_atlas is a native-only build tool (msdfgen is C++ FFI)");
}
