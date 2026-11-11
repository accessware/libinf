//! OS/2 help files are in the code page their NLS table names, and use IBM's glyphs below 32 too.

use encoding_rs::{BIG5, EUC_KR, Encoding, GBK, SHIFT_JIS, WINDOWS_1252};
use oem_cp::code_table::DECODING_TABLE_CP_MAP;

/// The code page a help file's text is decoded with, as IBM numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodePage(u16);

/// The IBM glyphs for the bytes below 32, which IPF text uses for arrows and symbols in syntax diagrams, where they would otherwise be control characters.
const LOW_GLYPHS: [char; 32] = [
	' ', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼', '►', '◄', '↕', '‼', '¶', '§', '▬',
	'↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];

impl CodePage {
	/// Multilingual Latin 1, OS/2's usual code page in Western Europe, which a file without an NLS table is read as.
	pub const DEFAULT: Self = Self(850);

	pub(crate) const fn new(number: u16) -> Self {
		Self(number)
	}

	/// The code page's IBM number, such as 850 or 932.
	#[must_use]
	pub const fn number(self) -> u16 {
		self.0
	}

	/// The double-byte and Windows code pages, which the WHATWG encodings cover; IBM's numbers for them differ from Microsoft's.
	fn whatwg(self) -> Option<&'static Encoding> {
		match self.0 {
			932 | 942 | 943 => Some(SHIFT_JIS),
			949 | 1363 => Some(EUC_KR),
			938 | 948 | 950 | 1370 => Some(BIG5),
			936 | 946 | 1381 | 1386 => Some(GBK),
			1004 | 1252 => Some(WINDOWS_1252),
			_ => None,
		}
	}

	/// The single-byte IBM code pages, with 850 standing in for one `oem_cp` lacks.
	fn decode_oem(self, bytes: &[u8]) -> String {
		let table = DECODING_TABLE_CP_MAP.get(&self.0).or_else(|| DECODING_TABLE_CP_MAP.get(&Self::DEFAULT.0));
		table.map_or_else(|| String::from_utf8_lossy(bytes).into_owned(), |table| table.decode_string_lossy(bytes))
	}

	pub(crate) fn decode(self, bytes: &[u8]) -> String {
		let text = self.whatwg().map_or_else(
			|| self.decode_oem(bytes),
			|encoding| encoding.decode_without_bom_handling(bytes).0.into_owned(),
		);
		text.chars().map(|character| LOW_GLYPHS.get(character as usize).copied().unwrap_or(character)).collect()
	}
}
