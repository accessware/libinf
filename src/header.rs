//! The 155-byte header at the start of every help file, which locates every other table.
//!
//! Field offsets follow Carl Hauser and Marcus Gröber's reverse-engineered description of
//! the format (version 2B, 1994). IBM never published one.

use crate::{
	InfError, Result,
	bytes::{slice, u8_at, u16_at, u32_at},
};

/// Whether a file is a stand-alone book or an application's help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
	/// A book read on its own with VIEW, such as a manual (`.inf`).
	Book,
	/// An application's help, opened from the program (`.hlp`).
	Help,
}

#[derive(Debug, Clone)]
pub struct Header {
	pub kind: Kind,
	pub toc_count: u16,
	pub toc_start: u32,
	pub index_count: u16,
	pub index_start: u32,
	pub slot_count: u16,
	pub slots_start: u32,
	pub dict_count: u16,
	pub dict_start: u32,
	pub title: Vec<u8>,
}

impl Header {
	pub fn read(data: &[u8]) -> Result<Self> {
		if !data.starts_with(b"HSP") {
			return Err(InfError::BadMagic);
		}
		let flags = u8_at(data, 3, "the header")?;
		let field16 = |at| u16_at(data, at, "the header");
		let field32 = |at| u32_at(data, at, "the header");
		let title = slice(data, 107, 48, "the header")?;
		let title_len = title.iter().position(|&byte| byte == 0).unwrap_or(title.len());
		Ok(Self {
			// Bit 4 marks a help file; a book has bit 0. A file with neither is read as a book, as VIEW does.
			kind: if flags & 0x10 != 0 { Kind::Help } else { Kind::Book },
			toc_count: field16(8)?,
			toc_start: field32(18)?,
			index_count: field16(34)?,
			index_start: field32(36)?,
			slot_count: field16(62)?,
			slots_start: field32(64)?,
			dict_count: field16(72)?,
			dict_start: field32(74)?,
			title: title[..title_len].to_vec(),
		})
	}
}
