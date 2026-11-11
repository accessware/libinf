//! A pure-Rust reader for OS/2 Information Presentation Facility help files: books
//! (`.inf`) and application help (`.hlp`).
//!
//! ```no_run
//! use libinf::InfFile;
//!
//! # fn main() -> libinf::Result<()> {
//! let book = InfFile::open("cmdref.inf")?;
//! println!("{}", book.title().unwrap_or("untitled"));
//! for (index, entry) in book.toc().iter().enumerate() {
//!     println!("== {} ==", entry.title);
//!     println!("{}", book.panel(index)?.text());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # What a help file is
//!
//! IBM's IPFC compiler turns IPF markup into one binary file. Its table of contents lists
//! every panel, hidden ones included, and each entry names the slots that hold its text.
//! Text is stored as numbers: a slot's bytes index its own short dictionary, which in turn
//! indexes the file's dictionary of every distinct word. Bytes past the slot's dictionary
//! mark paragraph ends, line breaks and spaces, and 0xff opens an escape sequence for
//! styles, links and examples.
//!
//! IBM never documented the compiled format. This crate follows Carl Hauser and Marcus
//! Gröber's reverse-engineered description (version 2B, 1994), checked against real files.
//!
//! OS/2 `.hlp` files are unrelated to WinHelp's, despite the shared extension, and `.inf`
//! is also the extension of Windows setup files. Both start with the bytes `HSP`, which
//! [`InfFile`] checks, rejecting anything else with [`InfError::BadMagic`].
//!
//! # Robustness
//!
//! Help files are untrusted input. Every offset and length is bounds checked before use.

#![warn(missing_docs)]

mod bytes;
mod decode;
mod encoding;
mod error;
mod header;
mod inf;
mod panel;

pub use encoding::CodePage;
pub use error::{InfError, Result};
pub use header::Kind;
pub use inf::InfFile;
pub use panel::{Block, BlockKind, ImageRef, IndexEntry, Link, Panel, Run, Style, TocEntry};

/// Whether `data` starts like an IPF help file, to tell one from a Windows setup file or a WinHelp file sharing its extension.
#[must_use]
pub fn is_ipf(data: &[u8]) -> bool {
	data.starts_with(b"HSP")
}
