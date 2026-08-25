//! **The icon manifest is stated twice, and this is what makes the two agree.**
//!
//! `fonts/icon.py`'s `ICONS` list decides which glyphs are drawn and at which
//! codepoint - it is where the geometry is authored and the face is baked.
//! `src/font/mod.rs`'s [`HIGHBAY_ICONS`] restates the same `(name, codepoint)`
//! pairs so Rust can resolve a name. Nothing compiled either against the other.
//!
//! # Why the duplication is not a mistake to remove
//!
//! `icon.py` records the reason the codepoints are written down rather than
//! derived: they used to be `ICON_BASE + index`, "which is only correct while
//! the list is never inserted into - adding `screen` alphabetically would have
//! renumbered `table` from U+F802 to U+F803 - silently, in a face that had
//! already shipped". So both sides hold a deliberate, hand-maintained fact.
//! What was missing was anything that notices when they stop matching.
//!
//! # What drift would look like without this
//!
//! A codepoint present in Rust and absent from the bake resolves to a glyph the
//! atlas never drew, and the atlas renders an uncovered cell as a **visible
//! tofu box** with no finding anywhere - the same silent class the `.hbdef`
//! artifacts had before `hb-pack --mode compile` gave them a producer. A name
//! added to `icon.py` and not to Rust is quieter still: `highbay_codepoint`
//! answers `None`, which callers are told to report as a MISSING ASSET, so a
//! forgotten line is indistinguishable from an unshipped one.
//!
//! # Why a test rather than codegen
//!
//! Four entries, changed about once a year, in two files of one crate. Codegen
//! buys the same guarantee and costs a build script that parses Python; the
//! defect here is not that a human maintains two lists, it is that nothing
//! checked them. If the set grows past a handful, generate `HIGHBAY_ICONS` from
//! `ICONS` and delete this file - the check becomes structural at that point,
//! which is strictly better.

use std::collections::BTreeMap;
use std::path::Path;

use libmsdf::{HIGHBAY_ICONS, HIGHBAY_ICONS_BLOCK, highbay_codepoint};

/// The `ICONS = [...]` list out of `fonts/icon.py`, as `(name, codepoint)`.
///
/// Parsed rather than imported because one side is Python. The parse is
/// deliberately narrow - it reads the bracketed block after `ICONS = [` and
/// takes the first two fields of each tuple - so a change to the file's SHAPE
/// fails loudly here instead of silently matching nothing. The
/// `entries_were_actually_found` assertion below is the guard against a parse
/// that quietly reads zero.
fn baker_manifest() -> BTreeMap<String, u32> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts/icon.py");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let start = text
        .find("\nICONS = [")
        .unwrap_or_else(|| panic!("no `ICONS = [` in {} - the baker's manifest moved or was renamed", path.display()));
    let body = &text[start..];
    let end = body
        .find("\n]")
        .unwrap_or_else(|| panic!("`ICONS = [` in {} is never closed", path.display()));

    let mut out = BTreeMap::new();
    for line in body[..end].lines() {
        let line = line.trim();
        let Some(inner) = line.strip_prefix('(') else {
            continue;
        };
        let mut fields = inner.split(',');
        let (Some(name), Some(code)) = (fields.next(), fields.next()) else {
            continue;
        };
        let name = name.trim().trim_matches('"').trim_matches('\'');
        let code = code.trim();
        let Some(hex) = code.strip_prefix("0x").or_else(|| code.strip_prefix("0X")) else {
            panic!("{name}'s codepoint in icon.py is `{code}`, not the `0x....` this parse reads");
        };
        let value = u32::from_str_radix(hex, 16)
            .unwrap_or_else(|e| panic!("{name}'s codepoint `{code}` is not hex: {e}"));
        out.insert(name.to_string(), value);
    }
    out
}

#[test]
fn entries_were_actually_found() {
    // Without this, every assertion below passes vacuously the day the parse
    // stops matching - which is the failure mode a hand-rolled parse has.
    let baked = baker_manifest();
    assert!(
        baked.len() >= 4,
        "parsed only {} entries from icon.py - the parse broke, and every other \
         test in this file would have passed by reading nothing",
        baked.len()
    );
}

#[test]
fn every_icon_rust_resolves_is_one_the_baker_draws() {
    let baked = baker_manifest();
    let mut wrong = Vec::new();
    for &(name, ch) in HIGHBAY_ICONS {
        match baked.get(name) {
            None => wrong.push(format!(
                "`{name}` is in HIGHBAY_ICONS and NOT in icon.py's ICONS - Rust \
                 resolves it to U+{:04X}, a cell the face never drew, and the \
                 atlas renders an uncovered cell as a visible tofu box",
                ch as u32
            )),
            Some(&code) if code != ch as u32 => wrong.push(format!(
                "`{name}` is U+{:04X} in Rust and U+{code:04X} in icon.py - one \
                 of the two resolves to a glyph the other never drew",
                ch as u32
            )),
            Some(_) => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "{} icon(s) disagree between the baker and Rust:\n  {}\n\n\
         `fonts/icon.py`'s ICONS decides what is drawn and where; \
         `src/font/mod.rs`'s HIGHBAY_ICONS restates it so a name can be \
         resolved. They are two hand-maintained statements of one fact - fix \
         whichever is wrong, and note that a codepoint already shipped must \
         not be renumbered (icon.py says why).",
        wrong.len(),
        wrong.join("\n  ")
    );
}

#[test]
fn every_icon_the_baker_draws_is_one_rust_can_resolve() {
    let baked = baker_manifest();
    let missing: Vec<String> = baked
        .iter()
        .filter(|(name, _)| highbay_codepoint(name).is_none())
        .map(|(name, code)| format!("`{name}` (U+{code:04X})"))
        .collect();
    assert!(
        missing.is_empty(),
        "{} icon(s) are baked into the face and unreachable from Rust: {}\n\n\
         `highbay_codepoint` answers `None` for them, which callers are told to \
         report as a MISSING ASSET - so a line forgotten in HIGHBAY_ICONS is \
         indistinguishable from a glyph nobody drew.",
        missing.len(),
        missing.join(", ")
    );
}

#[test]
fn every_codepoint_sits_in_the_block_the_repo_owns() {
    let (lo, hi) = HIGHBAY_ICONS_BLOCK;
    let (lo, hi) = (lo as u32, hi as u32);
    for (name, &code) in &baker_manifest() {
        assert!(
            (lo..=hi).contains(&code),
            "`{name}` is U+{code:04X}, outside HIGHBAY_ICONS_BLOCK \
             (U+{lo:04X}..=U+{hi:04X}). Below it is Material's own range and \
             above it is MARKERS; a codepoint outside the block is one this \
             repo does not own."
        );
    }
}
