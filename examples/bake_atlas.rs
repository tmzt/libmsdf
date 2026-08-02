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
//! Every bake also picks up whatever the face defines in the **Private Use
//! Area**, which is where an icon font puts its glyphs. That is how
//! `libhbui`'s atlas gets its Material Symbols cells:
//!
//! ```text
//! cargo run -p libmsdf --features cpu-bake --example bake_atlas -- \
//!     deps/libmsdf/fonts/Roboto-Regular-ascii-msymbols.ttf \
//!     crates/libhbui/assets/roboto-msymbols-48.atlas 48 6.0
//! ```
//!
//! Plain Roboto defines no PUA glyph, so that step queues nothing for it and
//! re-baking `Roboto-Regular-ascii.ttf` still reproduces the shipped
//! `src/assets/roboto-ascii-48.atlas` byte for byte.

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
    builder.add_ascii();
    // Union with everything the SHAPER can emit for printable ASCII — GSUB
    // digit remaps and, crucially, the fi/fl/ffi/ffl ligatures, which only
    // appear for ADJACENT characters and would otherwise draw blank.
    builder.add_shaped_ascii();
    // Whatever the face defines in the Private Use Area, which is where an
    // icon font puts its glyphs — Material Symbols' `menu` is U+E5D2. Stated
    // as "this face's PUA coverage" rather than a list of names so the bake
    // tool never has to learn an icon set: a face that defines no PUA glyph
    // (plain Roboto) queues nothing and bakes byte-identically to before.
    builder.add_codepoint_range('\u{E000}', '\u{F8FF}');
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
