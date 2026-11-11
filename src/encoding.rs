//! OS/2 help files are in an OEM code page, and use its glyphs below 32 too.

use oem_cp::code_table::{DECODING_TABLE_CP437, DECODING_TABLE_CP850};

/// The code page a help file's text is decoded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodePage {
	/// The original IBM PC code page, United States.
	Cp437,
	/// Multilingual Latin 1, OS/2's usual code page in Western Europe.
	Cp850,
}

/// The IBM glyphs for the bytes below 32, which IPF text uses for arrows and symbols in syntax diagrams, where they would otherwise be control characters.
const LOW_GLYPHS: [char; 32] = [
	' ', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼', '►', '◄', '↕', '‼', '¶', '§', '▬',
	'↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];

impl CodePage {
	pub(crate) fn decode(self, bytes: &[u8]) -> String {
		let table = match self {
			Self::Cp437 => &DECODING_TABLE_CP437,
			Self::Cp850 => &DECODING_TABLE_CP850,
		};
		bytes
			.iter()
			.map(|&byte| match byte {
				0..=31 => LOW_GLYPHS[usize::from(byte)],
				32..=127 => char::from(byte),
				_ => table[usize::from(byte - 128)],
			})
			.collect()
	}
}
