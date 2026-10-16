//! **The bundled fonts**: every face libmsdf can compile in, each behind its
//! own cargo feature, off by default, whose name carries the face's licence
//! (`docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`, section 4.4).
//! The TTF constants are defined in `libmsdf-font` (`libmsdf::font::faces`)
//! and re-exported here and at the crate root.
//!
//! With no font feature enabled the compiled crate contains no font data and
//! is licensed purely under MIT OR Apache-2.0 (README.md, "Licensing").

#[cfg(all(feature = "font-ofl-grenze-gotisch", feature = "shaper"))]
pub use crate::font::faces::GRENZE_GOTISCH_TTF;
#[cfg(all(feature = "font-apache-roboto-msymbols", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_ASCII_MSYMBOLS;
#[cfg(all(feature = "font-apache-roboto-bold", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_BOLD_ASCII;
#[cfg(all(feature = "font-apache-roboto-italic", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_ITALIC_ASCII;
#[cfg(all(feature = "font-apache-roboto-regular", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_REGULAR_ASCII;
#[cfg(all(feature = "font-ofl-texturina", feature = "shaper"))]
pub use crate::font::faces::TEXTURINA_TTF;
