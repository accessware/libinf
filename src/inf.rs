use std::{fs, path::Path};

use crate::{
	InfError, Result,
	bytes::{offset, slice, u8_at, u16_at, u32_at},
	decode::{self, Context, Slot},
	encoding::CodePage,
	header::{Header, Kind},
	panel::{ImageRef, IndexEntry, Panel, TocEntry},
	picture::{self, Picture},
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
	names: Vec<(String, usize)>,
	resources: Vec<(u16, usize)>,
	linked_files: Vec<String>,
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
		let names = read_names(&data, &header, &dictionary);
		let resources = read_resources(&data, &header);
		let linked_files = read_linked_files(&data, &header, code_page);
		Ok(Self { data, header, code_page, title, dictionary, toc, slots, names, resources, linked_files })
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

	/// The panels that have a name (`id` or `name` in the IPF source), which links from other files and applications use, with each one's index in the table of contents.
	#[must_use]
	pub fn names(&self) -> &[(String, usize)] {
		&self.names
	}

	/// The index in the table of contents of the panel named `name`, ignoring case as IPF does.
	#[must_use]
	pub fn panel_by_name(&self, name: &str) -> Option<usize> {
		self.names.iter().find(|(candidate, _)| candidate.eq_ignore_ascii_case(name)).map(|&(_, index)| index)
	}

	/// The panels that have a resource number (`res` in the IPF source), which applications open help by, with each one's index in the table of contents.
	#[must_use]
	pub fn resources(&self) -> &[(u16, usize)] {
		&self.resources
	}

	/// The index in the table of contents of the panel with resource number `resource`.
	#[must_use]
	pub fn panel_by_resource(&self, resource: u16) -> Option<usize> {
		self.resources.iter().find(|&&(candidate, _)| candidate == resource).map(|&(_, index)| index)
	}

	/// The other help files this one links into, as their names are written in it.
	#[must_use]
	pub fn linked_files(&self) -> &[String] {
		&self.linked_files
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

	/// The picture a panel places at `image`.
	///
	/// # Errors
	///
	/// Returns an error if the picture runs past the end of the file, or is stored in a way this crate can't decode.
	pub fn picture(&self, image: ImageRef) -> Result<Picture> {
		let at = offset(self.header.image_start)
			.checked_add(offset(image.offset))
			.ok_or(InfError::Truncated("a picture"))?;
		picture::read(&self.data, at)
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
	fn decode_text(&self, bytes: &[u8]) -> String {
		self.code_page.decode(bytes).trim().to_string()
	}

	fn linked_file(&self, index: usize) -> Option<&str> {
		self.linked_files.get(index).map(String::as_str)
	}
}

/// Two arrays of the same length: first the keys, then the table of contents index each leads to. A damaged table yields what can be read of it, since it only matters for links from outside.
fn read_pairs(data: &[u8], start: u32, count: u16) -> Vec<(u16, usize)> {
	let start = offset(start);
	let count = usize::from(count);
	(0..count)
		.map_while(|at| {
			let key = u16_at(data, start + at * 2, "a panel table").ok()?;
			let index = u16_at(data, start + (count + at) * 2, "a panel table").ok()?;
			Some((key, usize::from(index)))
		})
		.collect()
}

/// Each name is stored once, as a word in the dictionary.
fn read_names(data: &[u8], header: &Header, dictionary: &[String]) -> Vec<(String, usize)> {
	read_pairs(data, header.name_start, header.name_count)
		.into_iter()
		.filter_map(|(word, index)| Some((dictionary.get(usize::from(word))?.clone(), index)))
		.collect()
}

fn read_resources(data: &[u8], header: &Header) -> Vec<(u16, usize)> {
	read_pairs(data, header.resource_start, header.resource_count)
}

/// The extended data block gives the count and offset of the list of other files links lead to, after the font list's. Each name has a length byte that counts itself.
fn read_linked_files(data: &[u8], header: &Header, code_page: CodePage) -> Vec<String> {
	let block = offset(header.extended_start);
	if block == 0 {
		return Vec::new();
	}
	let (Ok(count), Ok(start)) =
		(u16_at(data, block + 6, "the extended data"), u32_at(data, block + 8, "the extended data"))
	else {
		return Vec::new();
	};
	let mut at = offset(start);
	let mut files = Vec::new();
	for _ in 0..count {
		let Ok(len) = u8_at(data, at, "the list of linked files") else { break };
		let Ok(name) = slice(data, at + 1, usize::from(len.saturating_sub(1)), "the list of linked files") else {
			break;
		};
		files.push(code_page.decode(name).trim().to_string());
		at += usize::from(len.max(1));
	}
	files
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

#[cfg(test)]
mod tests {
	use super::*;

	/// Lays out a two-panel book the way IPFC does, patching each table's place into the header as it goes.
	struct Writer(Vec<u8>);

	impl Writer {
		fn here(&self) -> u32 {
			u32::try_from(self.0.len()).unwrap()
		}

		fn set16(&mut self, at: usize, value: u16) {
			self.0[at..at + 2].copy_from_slice(&value.to_le_bytes());
		}

		fn set32(&mut self, at: usize, value: u32) {
			self.0[at..at + 4].copy_from_slice(&value.to_le_bytes());
		}

		fn push16(&mut self, values: &[u16]) {
			for value in values {
				self.0.extend_from_slice(&value.to_le_bytes());
			}
		}

		/// Strings with a length byte that counts itself, as the dictionary and the list of linked files store them.
		fn push_counted(&mut self, strings: &[&str]) {
			for string in strings {
				self.0.push(u8::try_from(string.len() + 1).unwrap());
				self.0.extend_from_slice(string.as_bytes());
			}
		}
	}

	fn book() -> Vec<u8> {
		let mut file = Writer(vec![0; 155]);
		file.0[..4].copy_from_slice(b"HSP\x01");
		file.0[107..116].copy_from_slice(b"Test book");
		file.set32(72 + 2, file.here());
		file.set16(72, 4);
		file.push_counted(&["Hello", "world", "Second", "PANEL_TWO"]);
		let mut slots = Vec::new();
		for (words, text) in [(&[0_u16, 1][..], &[0_u8, 1][..]), (&[2][..], &[0][..])] {
			let dictionary = file.here();
			file.push16(words);
			slots.push(file.here());
			file.0.push(0);
			file.0.extend_from_slice(&dictionary.to_le_bytes());
			file.0.push(u8::try_from(words.len()).unwrap());
			file.push16(&[u16::try_from(text.len()).unwrap()]);
			file.0.extend_from_slice(text);
		}
		file.set16(62, 2);
		file.set32(64, file.here());
		for slot in slots {
			file.0.extend_from_slice(&slot.to_le_bytes());
		}
		let mut entries = Vec::new();
		for (flags, slot, title) in [(0x81_u8, 0_u16, "One"), (0x42, 1, "Two")] {
			entries.push(file.here());
			file.0.extend_from_slice(&[u8::try_from(5 + title.len()).unwrap(), flags, 1]);
			file.push16(&[slot]);
			file.0.extend_from_slice(title.as_bytes());
		}
		file.set16(8, 2);
		file.set32(18, file.here());
		for entry in entries {
			file.0.extend_from_slice(&entry.to_le_bytes());
		}
		file.set16(22, 1);
		file.set32(24, file.here());
		file.push16(&[42, 1]);
		file.set16(28, 1);
		file.set32(30, file.here());
		file.push16(&[3, 1]);
		file.set16(34, 1);
		file.set32(36, file.here());
		file.0.extend_from_slice(&[8, 1, 0, 0, 0]);
		file.0.extend_from_slice(b"greeting");
		let files = file.here();
		file.push_counted(&["OTHER.INF"]);
		file.set32(91, file.here());
		let block = file.0.len();
		file.0.extend_from_slice(&[0; 64]);
		file.set16(block + 6, 1);
		file.set32(block + 8, files);
		file.0
	}

	#[test]
	fn a_book_reads_its_contents_and_tables() {
		let book = InfFile::from_bytes(book()).unwrap();
		assert_eq!(book.title(), Some("Test book"));
		assert_eq!(book.kind(), Kind::Book);
		assert_eq!(book.code_page(), CodePage::DEFAULT);
		let toc: Vec<_> = book
			.toc()
			.iter()
			.map(|entry| (entry.title.as_str(), entry.level, entry.hidden, entry.has_children))
			.collect();
		assert_eq!(toc, [("One", 1, false, true), ("Two", 2, true, false)]);
		assert_eq!(book.panel(0).unwrap().text(), "Hello world");
		assert_eq!(book.panel(1).unwrap().text(), "Second");
		assert!(matches!(book.panel(2), Err(InfError::NoSuchEntry(2))));
		assert_eq!(book.panel_by_name("panel_two"), Some(1));
		assert_eq!(book.panel_by_resource(42), Some(1));
		assert_eq!(book.panel_by_resource(7), None);
		assert_eq!(book.linked_files(), ["OTHER.INF"]);
		let index = book.index().unwrap();
		assert_eq!((index[0].term.as_str(), index[0].panel, index[0].subentry), ("greeting", 0, false));
	}

	#[test]
	fn other_files_are_refused_and_damaged_ones_report_where() {
		assert!(matches!(InfFile::from_bytes(b"[Version]\r\n".to_vec()), Err(InfError::BadMagic)));
		assert!(!crate::is_ipf(b"?_\x03\x00"));
		let mut cut = book();
		cut.truncate(200);
		assert!(matches!(InfFile::from_bytes(cut), Err(InfError::Truncated(_))));
	}
}
