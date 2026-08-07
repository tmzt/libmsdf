//! Build-time atlas bake helper (native-only): font file → baked MSDF
//! atlas bytes (`FontAtlas::to_bytes` format, loadable on wasm32 where the
//! CPU msdfgen path doesn't exist).
//!
//! Usage:
//! ```text
//! cargo run -p libmsdf --example bake_atlas -- <font.ttf> <out.atlas> [glyph_size] [px_range]
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
//!     crates/libhbui/assets/roboto-msymbols-48.atlas 48 6.0
//! ```
//!
//! Both bundled faces carry the marker block ([`libmsdf::MARKERS`] — geometry
//! this repo DRAWS with, as opposed to the Material Symbols it borrows), so the
//! two bakes differ only by the nine icon cells.
//!
//! The queue is append-only, so a re-bake after a coverage widening is a
//! strict SUPERSET of the previous one: every cell that existed keeps its
//! atlas coordinates and its glyph-table index, and the texture just gets
//! taller. A rendered frame that moves after a re-bake is a real finding.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args: Vec<String> = std::env::args().collect();

    let (font_data, out_path) = if args.len() >= 3 {
        let data = std::fs::read(&args[1])
            .unwrap_or_else(|e| panic!("read font {}: {e}", args[1]));
        (data, args[2].clone())
    } else {
        (
            libmsdf::ROBOTO_REGULAR_ASCII.to_vec(),
            "roboto-ascii-48.atlas".to_string(),
        )
    };
    let glyph_size: u32 = args.get(3).map(|s| s.parse().expect("glyph_size")).unwrap_or(48);
    let px_range: f64 = args.get(4).map(|s| s.parse().expect("px_range")).unwrap_or(6.0);

    let mut builder = libmsdf::FontAtlasBuilder::new(font_data, glyph_size, px_range);
    builder.add_shipped_coverage();
    let atlas = builder.build().expect("atlas bake failed");
    let bytes = atlas.to_bytes();
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
