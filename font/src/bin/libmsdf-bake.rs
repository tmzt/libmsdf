//! **`libmsdf-bake`**: the baker as a command (bundled fonts plan, section
//! 4.6).
//!
//! ```text
//! cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- \
//!     face <font.ttf> <out.atlas> [--size 48] [--px-range 6.0] [--columns 8]
//!          [--style <bold|italic|bolditalic>=<face.ttf>]...
//!          [--layout] [--feature <tag>[@<style>,...]]...
//!          [--licence <SPDX>=<source>[@<style>,...]]...
//! cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- \
//!     bundled [--fonts <dir>] [--out <dir>] [--check <dir>] [--hashes]
//! ```
//!
//! `bundled` bakes every bundled atlas into `<dir>` (default `out/bundled/`),
//! with each atlas's bake report beside it; `--check <dir>` bakes into memory
//! and compares with the files in `<dir>`; `--hashes` prints lines in the
//! `fonts/bundled.sha256` format for this host.

use std::path::{Path, PathBuf};
use std::process::exit;

use libmsdf_font::bake::{BakeOptions, Baker, LicenceSpec, sha256};
use libmsdf_font::bundled::hex_of;
use libmsdf_font::{BundledAtlas, GlyphStyle, LayoutOptions};

fn style_of(name: &str) -> GlyphStyle {
    match name.to_ascii_lowercase().as_str() {
        "regular" => GlyphStyle::Regular,
        "bold" => GlyphStyle::Bold,
        "italic" => GlyphStyle::Italic,
        "bolditalic" => GlyphStyle::BoldItalic,
        other => {
            eprintln!("libmsdf-bake: unknown style {other:?}");
            exit(2)
        }
    }
}

fn styles_of(list: Option<&str>) -> Vec<GlyphStyle> {
    list.map_or(vec![GlyphStyle::Regular], |l| {
        l.split(',').map(style_of).collect()
    })
}

/// The host triple this binary was built for, for `--hashes`.
fn host() -> &'static str {
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(
        target_arch = "x86_64",
        target_os = "linux",
        target_env = "gnu"
    )) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(
        target_arch = "aarch64",
        target_os = "linux",
        target_env = "gnu"
    )) {
        "aarch64-unknown-linux-gnu"
    } else {
        "unknown"
    }
}

fn face(args: &[String]) {
    let mut positional = Vec::new();
    let mut opts = BakeOptions::new(48, 6.0);
    let mut layout = false;
    let mut features: Vec<(GlyphStyle, [u8; 4])> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next().cloned().unwrap_or_else(|| {
                eprintln!("libmsdf-bake: {a} needs a value");
                exit(2)
            })
        };
        match a.as_str() {
            "--size" => opts.glyph_size = value().parse().expect("--size"),
            "--px-range" => opts.px_range = value().parse().expect("--px-range"),
            "--columns" => opts.columns = value().parse().expect("--columns"),
            "--layout" => layout = true,
            "--style" => {
                let v = value();
                let (name, path) = v.split_once('=').unwrap_or_else(|| {
                    eprintln!("libmsdf-bake: --style takes <style>=<face.ttf>");
                    exit(2)
                });
                opts.styles.push((style_of(name), PathBuf::from(path)));
            }
            "--feature" => {
                let v = value();
                let (tag, styles) = v
                    .split_once('@')
                    .map_or((v.as_str(), None), |(t, s)| (t, Some(s)));
                let tag: [u8; 4] = format!("{tag:<4}")
                    .as_bytes()
                    .try_into()
                    .unwrap_or_else(|_| {
                        eprintln!("libmsdf-bake: --feature takes a 4-letter tag");
                        exit(2)
                    });
                for s in styles_of(styles) {
                    features.push((s, tag));
                }
            }
            "--licence" => {
                let v = value();
                let (body, styles) = v
                    .split_once('@')
                    .map_or((v.as_str(), None), |(b, s)| (b, Some(s)));
                let (spdx, source) = body.split_once('=').unwrap_or_else(|| {
                    eprintln!("libmsdf-bake: --licence takes <SPDX>=<source>[@<style>,...]");
                    exit(2)
                });
                opts.licences.push(LicenceSpec {
                    spdx: spdx.into(),
                    source: source.into(),
                    styles: styles_of(styles),
                });
            }
            _ => positional.push(a.clone()),
        }
    }
    let [ttf, out] = positional.as_slice() else {
        eprintln!("libmsdf-bake face <font.ttf> <out.atlas> [options]");
        exit(2)
    };
    if layout {
        let mut styles = vec![GlyphStyle::Regular];
        styles.extend(opts.styles.iter().map(|s| s.0));
        opts.layout = Some(
            styles
                .into_iter()
                .map(|s| {
                    let lo = LayoutOptions {
                        extra_features: features.iter().filter(|f| f.0 == s).map(|f| f.1).collect(),
                        ..Default::default()
                    };
                    (s, lo)
                })
                .collect(),
        );
    }
    match Baker::bake_face_report(Path::new(ttf), &opts) {
        Ok((bytes, report)) => {
            std::fs::write(out, &bytes).expect("write atlas");
            print!("{report}");
            println!(
                "baked {out}: {} bytes, sha256 {}",
                bytes.len(),
                hex_of(&sha256(&bytes))
            );
        }
        Err(e) => {
            eprintln!("libmsdf-bake: FAILED - {e}");
            exit(1)
        }
    }
}

fn bundled(args: &[String]) {
    let mut fonts = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../fonts"));
    let mut out = PathBuf::from("out/bundled");
    let mut check: Option<PathBuf> = None;
    let mut hashes = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fonts" => fonts = PathBuf::from(it.next().expect("--fonts <dir>")),
            "--out" => out = PathBuf::from(it.next().expect("--out <dir>")),
            "--check" => check = Some(PathBuf::from(it.next().expect("--check <dir>"))),
            "--hashes" => hashes = true,
            other => {
                eprintln!("libmsdf-bake bundled: unknown argument {other:?}");
                exit(2)
            }
        }
    }
    let mut failed = false;
    for atlas in BundledAtlas::ALL {
        let started = std::time::Instant::now();
        let (bytes, report) = match Baker::bake_bundled_report(atlas, &fonts) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("libmsdf-bake: {}: FAILED - {e}", atlas.file_name());
                exit(1)
            }
        };
        let digest = hex_of(&sha256(&bytes));
        eprintln!(
            "{}: {} bytes, sha256 {digest}, baked in {:.2?}",
            atlas.file_name(),
            bytes.len(),
            started.elapsed()
        );
        if hashes {
            println!("{digest}  {}  {}", atlas.file_name(), host());
        }
        match &check {
            Some(dir) => {
                let path = dir.join(atlas.file_name());
                match std::fs::read(&path) {
                    Ok(committed) if committed == bytes => eprintln!("  OK - {}", path.display()),
                    Ok(_) => {
                        eprintln!("  STALE - {} differs from this bake", path.display());
                        failed = true;
                    }
                    Err(e) => {
                        eprintln!("  MISSING - {}: {e}", path.display());
                        failed = true;
                    }
                }
            }
            None => {
                std::fs::create_dir_all(&out).expect("create the output directory");
                std::fs::write(out.join(atlas.file_name()), &bytes).expect("write atlas");
                let report_name = atlas.file_name().replace(".atlas", ".report.txt");
                std::fs::write(out.join(report_name), report.to_string()).expect("write report");
            }
        }
    }
    if failed {
        exit(1)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("face") => face(&args[1..]),
        Some("bundled") => bundled(&args[1..]),
        _ => {
            eprintln!(
                "usage: libmsdf-bake face <font.ttf> <out.atlas> [options] | bundled [options]"
            );
            exit(2)
        }
    }
}
