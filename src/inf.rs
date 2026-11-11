use std::{fs, path::Path};

use crate::{
	InfError, Result,
	bytes::{offset, slice, u8_at, u16_at, u32_at},
	decode::{self, Context, Slot},
	encoding::CodePage,
	header::{Header, Kind},
	panel::{IndexEntry, Panel, TocEntry},
};

/// An OS/2 help file, read into memory.
#[derive(Debug, Clone)]
pub struct InfFile {
	data: Vec<u8>,
	header: Header,
	code_page: CodePage,
	title: String,
	dictionary: Vec<String>,
	toc: Vec<TocEntry>,
	slots: Vec<u32>,
}

/// The table of contents entry flags.
const HAS_CHILDREN: u8 = 0x80;
const HIDDEN: u8 = 0x40;
const EXTENDED: u8 = 0x20;
const LEVEL: u8 = 0x0f;

impl InfFile {
	/// Reads the help file at `path`.
	///
	/// # Errors
	///
	/// Returns an error if the file can't be read, is not an IPF help file, or its tables run past its end.
	pub fn open(path: impl AsRef<Path>) -> Result<Self> {
		Self::from_bytes(fs::read(path)?)
	}

	/// Reads a help file already in memory.
	///
	/// # Errors
	///
	/// Returns an error if `data` is not an IPF help file, or its tables run past its end.
	pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
		let header = Header::read(&data)?;
		let code_page = code_page(&data, &header);
		let title = code_page.decode(&header.title).trim().to_string();
		let dictionary = read_dictionary(&data, &header, code_page)?;
		let toc = read_toc(&data, &header, code_page)?;
		let slots = (0..usize::from(header.slot_count))
			.map(|slot| u32_at(&data, offset(header.slots_start) + slot * 4, "the slot table"))
			.collect::<Result<_>>()?;
		Ok(Self { data, header, code_page, title, dictionary, toc, slots })
	}

	/// Whether the file is a book (`.inf`) or an application's help (`.hlp`).
	#[must_use]
	pub const fn kind(&self) -> Kind {
		self.header.kind
	}

	/// The title the file was compiled with, if it has one.
	#[must_use]
	pub fn title(&self) -> Option<&str> {
		Some(self.title.as_str()).filter(|title| !title.is_empty())
	}

	/// The code page the file's text is decoded with.
	#[must_use]
	pub const fn code_page(&self) -> CodePage {
		self.code_page
	}

	/// The table of contents, which lists every panel, hidden ones included.
	#[must_use]
	pub fn toc(&self) -> &[TocEntry] {
		&self.toc
	}

	/// The content of the panel at `index` in the table of contents.
	///
	/// # Errors
	///
	/// Returns an error if there is no such entry, or its text runs past the end of the file.
	pub fn panel(&self, index: usize) -> Result<Panel> {
		let entry = self.toc.get(index).ok_or(InfError::NoSuchEntry(index))?;
		let slots = entry.slots.iter().map(|&slot| self.slot(slot)).collect::<Result<Vec<_>>>()?;
		Ok(Panel { blocks: decode::decode(&slots, self) })
	}

	fn slot(&self, number: u16) -> Result<Slot<'_>> {
		let start = offset(*self.slots.get(usize::from(number)).ok_or(InfError::Truncated("the slot table"))?);
		let dict_start = offset(u32_at(&self.data, start + 1, "a panel")?);
		let word_count = u8_at(&self.data, start + 5, "a panel")?;
		let text_len = u16_at(&self.data, start + 6, "a panel")?;
		let text = slice(&self.data, start + 8, usize::from(text_len), "a panel")?;
		let words = (0..usize::from(word_count))
			.map(|word| {
				let entry = u16_at(&self.data, dict_start + word * 2, "a panel's dictionary")?;
				Ok(self.dictionary.get(usize::from(entry)).map_or("", String::as_str))
			})
			.collect::<Result<_>>()?;
		Ok(Slot { text, words })
	}

	/// The book's index, in the order the file stores it, which is alphabetical.
	///
	/// # Errors
	///
	/// Returns an error if the index runs past the end of the file.
	pub fn index(&self) -> Result<Vec<IndexEntry>> {
		let mut entries = Vec::with_capacity(usize::from(self.header.index_count));
		let mut at = offset(self.header.index_start);
		for _ in 0..self.header.index_count {
			let len = usize::from(u8_at(&self.data, at, "the index")?);
			let level = u8_at(&self.data, at + 1, "the index")?;
			let panel = usize::from(u16_at(&self.data, at + 3, "the index")?);
			let term = slice(&self.data, at + 5, len, "the index")?;
			entries.push(IndexEntry { term: self.code_page.decode(term), subentry: level & 0x0f > 1, panel });
			at += 5 + len;
		}
		Ok(entries)
	}
}

impl Context for InfFile {
	fn toc_title(&self, index: usize) -> Option<&str> {
		self.toc.get(index).map(|entry| entry.title.as_str())
	}
}

/// The NLS table opens with its size (two bytes), a type, a format and a value (two bytes), then the country and code page the book was written in.
fn code_page(data: &[u8], header: &Header) -> CodePage {
	if header.nls_len < 12 {
		return CodePage::DEFAULT;
	}
	u16_at(data, offset(header.nls_start) + 8, "the NLS table").map_or(CodePage::DEFAULT, CodePage::new)
}

/// Each word is stored with a length byte that counts itself.
fn read_dictionary(data: &[u8], header: &Header, code_page: CodePage) -> Result<Vec<String>> {
	let mut words = Vec::with_capacity(usize::from(header.dict_count));
	let mut at = offset(header.dict_start);
	for _ in 0..header.dict_count {
		let len = usize::from(u8_at(data, at, "the dictionary")?);
		let word = slice(data, at + 1, len.saturating_sub(1), "the dictionary")?;
		words.push(code_page.decode(word));
		at += len.max(1);
	}
	Ok(words)
}

fn read_toc(data: &[u8], header: &Header, code_page: CodePage) -> Result<Vec<TocEntry>> {
	(0..usize::from(header.toc_count))
		.map(|index| {
			let start = offset(u32_at(data, offset(header.toc_start) + index * 4, "the table of contents")?);
			let len = usize::from(u8_at(data, start, "the table of contents")?);
			let entry = slice(data, start, len, "the table of contents")?;
			let flags = *entry.get(1).ok_or(InfError::Truncated("the table of contents"))?;
			let slot_count = usize::from(*entry.get(2).ok_or(InfError::Truncated("the table of contents"))?);
			let mut at = 3;
			if flags & EXTENDED != 0 {
				// An extended entry describes the panel's window first; how much there is depends on its first two bytes.
				let w1 = *entry.get(3).ok_or(InfError::Truncated("the table of contents"))?;
				let w2 = *entry.get(4).ok_or(InfError::Truncated("the table of contents"))?;
				at = 5;
				if w1 & 0x08 != 0 {
					at += 2;
				}
				if w1 & 0x01 != 0 {
					at += 5;
				}
				if w1 & 0x02 != 0 {
					at += 5;
				}
				if w2 & 0x04 != 0 {
					at += 2;
				}
			}
			let slots = (0..slot_count)
				.map(|slot| u16_at(entry, at + slot * 2, "the table of contents"))
				.collect::<Result<Vec<_>>>()?;
			let title = entry.get(at + slot_count * 2..).unwrap_or_default();
			Ok(TocEntry {
				title: code_page.decode(title).trim().to_string(),
				level: flags & LEVEL,
				hidden: flags & HIDDEN != 0,
				has_children: flags & HAS_CHILDREN != 0,
				slots,
			})
		})
		.collect()
}
