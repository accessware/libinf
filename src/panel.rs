//! What a panel holds once decoded: blocks of runs, each with its style and link.

/// One entry in the table of contents, which in an IPF file is also the list of panels: every panel is reached through its entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
	/// The heading VIEW shows for the panel.
	pub title: String,
	/// The nesting level, from 1 for a top-level heading. IPF allows up to 6.
	pub level: u8,
	/// Whether the entry is left out of the table of contents VIEW shows. A hidden panel is still reached through links, as footnotes and pop-ups are.
	pub hidden: bool,
	/// Whether the entries that follow at a deeper level belong under this one.
	pub has_children: bool,
	pub(crate) slots: Vec<u16>,
}

/// A panel's content: the text under one table of contents entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Panel {
	/// The panel's paragraphs and examples, in reading order.
	pub blocks: Vec<Block>,
}

impl Panel {
	/// The panel as plain text, a blank line between blocks.
	#[must_use]
	pub fn text(&self) -> String {
		self.blocks.iter().map(Block::text).filter(|text| !text.trim().is_empty()).collect::<Vec<_>>().join("\n\n")
	}
}

/// How a block's text is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
	/// Running text that VIEW wraps to the window.
	Paragraph,
	/// A monospaced example (`:xmp.`), whose lines and spacing are kept as written.
	Example,
	/// Lines kept as written but not monospaced (`:lines.`).
	Lines,
}

/// A paragraph or example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
	/// How the block's text is laid out.
	pub kind: BlockKind,
	/// The pieces the block is built from, in reading order.
	pub runs: Vec<Run>,
}

impl Block {
	/// The block's text, with line breaks as their own characters and pictures left out.
	#[must_use]
	pub fn text(&self) -> String {
		let mut text = String::new();
		for run in &self.runs {
			match run {
				Run::Text { text: run_text, .. } => text.push_str(run_text),
				Run::LineBreak => text.push('\n'),
				Run::Image(_) => {}
			}
		}
		text
	}
}

/// A piece of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
	/// A stretch of text with one style, inside at most one link.
	Text {
		/// The text.
		text: String,
		/// Its emphasis.
		style: Style,
		/// The link it is part of, if any.
		link: Option<Link>,
	},
	/// A line break inside the block.
	LineBreak,
	/// A picture.
	Image(ImageRef),
}

/// Emphasis, as set by IPF's `:hp1.` to `:hp7.` tags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
	/// Bold text.
	pub bold: bool,
	/// Italic text.
	pub italic: bool,
	/// Underlined text.
	pub underline: bool,
}

/// What a link leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Link {
	/// Another panel, by its index in the table of contents.
	Panel(usize),
	/// A footnote, a pop-up panel, by its index in the table of contents.
	Footnote(usize),
	/// A program the link starts (`reftype=launch`), with its command line, which often opens a web page: `explore.exe http://...`.
	Launch(String),
	/// A message to the application showing the help (`reftype=inform`), which only means something inside that application.
	Inform(u16),
	/// A panel in another help file (`reftype=hd database=...`).
	OtherFile {
		/// The other file's name, as the link gives it, usually without a folder.
		file: Option<String>,
		/// The name (`id` or `name`) of the panel it leads to, which [`InfFile::panel_by_name`](crate::InfFile::panel_by_name) finds once that file is open.
		panel: String,
	},
}

/// A picture placed in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageRef {
	/// Where the picture's data starts, from the start of the file's image area.
	pub offset: u32,
}

/// An entry in the book's index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
	/// The term.
	pub term: String,
	/// Whether the term is a second-level entry under the one before it.
	pub subentry: bool,
	/// The panel it leads to, by its index in the table of contents.
	pub panel: usize,
}
