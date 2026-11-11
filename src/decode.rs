//! Turns a panel's encoded text into blocks.
//!
//! A panel's text is a stream of bytes. One below the slot's local dictionary size names a
//! word; the rest are layout codes, and 0xff opens an escape sequence that sets a style,
//! starts or ends a link, or switches between running text and examples.

use crate::panel::{Block, BlockKind, ImageRef, Link, Run, Style};

/// End of paragraph.
const PARAGRAPH: u8 = 0xfa;
/// Turns the space after each word off or back on.
const TOGGLE_SPACING: u8 = 0xfc;
const LINE_BREAK: u8 = 0xfd;
const SPACE: u8 = 0xfe;
const ESCAPE: u8 = 0xff;

/// One slot's encoded text, with the words its local dictionary names already decoded.
pub struct Slot<'a> {
	pub text: &'a [u8],
	pub words: Vec<&'a str>,
}

/// What [`decode`] needs from the rest of the file.
pub trait Context {
	/// Text in the file's code page, as a link's command line or target is.
	fn decode_text(&self, bytes: &[u8]) -> String;
	/// The name of another help file this one links into, by its number in the file's list.
	fn linked_file(&self, index: usize) -> Option<&str>;
}

#[derive(Default)]
struct Builder {
	blocks: Vec<Block>,
	kind: Option<BlockKind>,
	runs: Vec<Run>,
	text: String,
	style: Style,
	link: Option<Link>,
	example: Option<BlockKind>,
	hidden: bool,
}

impl Builder {
	fn kind(&self) -> BlockKind {
		self.example.unwrap_or(BlockKind::Paragraph)
	}

	fn push_text(&mut self, text: &str) {
		if self.hidden || text.is_empty() {
			return;
		}
		if self.text.is_empty() && self.runs.is_empty() && self.kind.is_none() {
			self.kind = Some(self.kind());
		}
		self.text.push_str(text);
	}

	fn flush_text(&mut self) {
		if !self.text.is_empty() {
			let text = std::mem::take(&mut self.text);
			self.runs.push(Run::Text { text, style: self.style, link: self.link.clone() });
		}
	}

	/// A style or link change starts a new run, so each run has one of each.
	fn set_style(&mut self, style: Style) {
		if style != self.style {
			self.flush_text();
			self.style = style;
		}
	}

	fn set_link(&mut self, link: Option<Link>) {
		self.flush_text();
		self.link = link;
	}

	/// The space the last word left after it would end the line, so it is dropped.
	fn line_break(&mut self) {
		if self.hidden {
			return;
		}
		self.flush_text();
		self.trim_last_run();
		if !self.runs.is_empty() {
			self.runs.push(Run::LineBreak);
		}
	}

	fn trim_last_run(&mut self) {
		if let Some(Run::Text { text, .. }) = self.runs.last_mut() {
			let trimmed = text.trim_end_matches(' ').len();
			text.truncate(trimmed);
		}
	}

	fn image(&mut self, image: ImageRef) {
		if self.hidden {
			return;
		}
		self.flush_text();
		let kind = self.kind();
		self.kind.get_or_insert(kind);
		self.runs.push(Run::Image(image));
	}

	/// Ends the block, dropping the space each word leaves after it and line breaks at its end. A paragraph's leading spaces go too, since VIEW wraps running text to the window; an example's are its indentation.
	fn finish_block(&mut self) {
		self.flush_text();
		while matches!(self.runs.last(), Some(Run::LineBreak)) {
			self.runs.pop();
		}
		self.trim_last_run();
		let kind = self.kind.take().unwrap_or(BlockKind::Paragraph);
		if kind == BlockKind::Paragraph
			&& let Some(Run::Text { text, .. }) = self.runs.first_mut()
		{
			let start = text.len() - text.trim_start_matches(' ').len();
			text.drain(..start);
		}
		if self.runs.iter().any(|run| !matches!(run, Run::Text { text, .. } if text.is_empty())) {
			self.blocks.push(Block { kind, runs: std::mem::take(&mut self.runs) });
		}
		self.runs.clear();
		self.kind = None;
	}
}

/// The style a `:hp.` escape's argument selects.
const fn style(code: u8) -> Style {
	match code {
		1 => Style { italic: true, bold: false, underline: false },
		2 => Style { bold: true, italic: false, underline: false },
		3 => Style { bold: true, italic: true, underline: false },
		4 => Style { underline: true, bold: false, italic: false },
		5 => Style { italic: true, underline: true, bold: false },
		6 => Style { bold: true, underline: true, italic: false },
		_ => Style { bold: false, italic: false, underline: false },
	}
}

fn arg16(args: &[u8], at: usize) -> Option<usize> {
	Some(usize::from(u16::from_le_bytes([*args.get(at)?, *args.get(at + 1)?])))
}

/// Whether the compiler left spacing off for the whole slot, which happens when an earlier panel opens an example and never closes it: the slot then has no toggles and puts an explicit space after most words.
fn spaced_explicitly(slot: &Slot<'_>) -> bool {
	let is_word = |byte: u8| usize::from(byte) < slot.words.len();
	let (mut spaced, mut abutted) = (0, 0);
	let mut index = 0;
	while let Some(&byte) = slot.text.get(index) {
		match byte {
			TOGGLE_SPACING => return false,
			ESCAPE => index += usize::from(slot.text.get(index + 1).copied().unwrap_or(0)),
			_ if is_word(byte) => match slot.text.get(index + 1) {
				Some(&SPACE) => spaced += 1,
				Some(&next) if is_word(next) => abutted += 1,
				_ => {}
			},
			_ => {}
		}
		index += 1;
	}
	spaced >= 3 && spaced > abutted
}

/// Decodes a panel made of `slots`, in order.
pub fn decode(slots: &[Slot<'_>], context: &dyn Context) -> Vec<Block> {
	let mut builder = Builder::default();
	for slot in slots {
		// Every slot starts with words spaced, unless it continues an example.
		let explicit = spaced_explicitly(slot);
		let mut spacing = builder.example != Some(BlockKind::Example) && !explicit;
		let mut index = 0;
		while let Some(&byte) = slot.text.get(index) {
			index += 1;
			if let Some(word) = slot.words.get(usize::from(byte)) {
				builder.push_text(word);
				if spacing && *word != " " {
					builder.push_text(" ");
				}
				continue;
			}
			match byte {
				PARAGRAPH => {
					builder.finish_block();
					spacing = builder.example != Some(BlockKind::Example) && !explicit;
				}
				TOGGLE_SPACING => spacing = !spacing,
				LINE_BREAK => {
					builder.line_break();
					spacing = builder.example != Some(BlockKind::Example) && !explicit;
				}
				SPACE => builder.push_text(" "),
				ESCAPE => {
					// The length counts itself and the code but not the 0xff.
					let Some(&len) = slot.text.get(index) else { break };
					let len = usize::from(len);
					let Some(sequence) = slot.text.get(index..index + len) else { break };
					index += len;
					if let [_, code, args @ ..] = sequence {
						escape(&mut builder, &mut spacing, *code, args, context);
					}
				}
				// 0xfb is unknown, and a word number past the local dictionary means a damaged slot; both are skipped.
				_ => {}
			}
		}
	}
	builder.finish_block();
	builder.blocks
}

fn escape(builder: &mut Builder, spacing: &mut bool, code: u8, args: &[u8], context: &dyn Context) {
	match code {
		// 0x11 sets the left margin on a new line.
		0x11 => builder.line_break(),
		0x04 | 0x19 => builder.set_style(style(args.first().copied().unwrap_or(0))),
		0x05 => builder.set_link(arg16(args, 0).map(Link::Panel)),
		0x07 => builder.set_link(arg16(args, 0).map(Link::Footnote)),
		0x08 => builder.set_link(None),
		0x0b => {
			builder.finish_block();
			builder.example = Some(BlockKind::Example);
			*spacing = false;
		}
		0x0c | 0x1b => {
			builder.finish_block();
			builder.example = None;
			*spacing = true;
		}
		0x0e => {
			if let Some(offset) = args.get(1..5) {
				builder.image(ImageRef { offset: u32::from_le_bytes([offset[0], offset[1], offset[2], offset[3]]) });
			}
		}
		// The first byte's meaning is unknown; the command line follows.
		0x10 => builder.set_link(Some(Link::Launch(
			args.get(1..).map_or_else(String::new, |command| context.decode_text(command)),
		))),
		0x16 => builder
			.set_link(Some(Link::Inform(arg16(args, 0).and_then(|value| u16::try_from(value).ok()).unwrap_or(0)))),
		// The other file's number in the list, then the target panel's name with its length.
		0x1f => {
			let name = args.get(1).and_then(|&len| args.get(2..2 + usize::from(len)));
			builder.set_link(Some(Link::OtherFile {
				file: args.first().and_then(|&index| context.linked_file(usize::from(index))).map(ToString::to_string),
				panel: name.map_or_else(String::new, |name| context.decode_text(name)),
			}));
		}
		0x17 => {
			builder.flush_text();
			builder.hidden = true;
		}
		0x18 => builder.hidden = false,
		0x1a => {
			builder.finish_block();
			builder.example = Some(BlockKind::Lines);
			*spacing = false;
		}
		// 0x0f opens a child window when the panel is shown, or with only three bytes puts a panel's title in the text, which real files only use beside child windows, where the title would be noise. Margins, colours, fonts and the rest only change how VIEW draws the text.
		_ => {}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::panel::{Run, Style};

	struct Book;

	impl Context for Book {
		fn decode_text(&self, bytes: &[u8]) -> String {
			String::from_utf8_lossy(bytes).into_owned()
		}

		fn linked_file(&self, index: usize) -> Option<&str> {
			(index == 0).then_some("OTHER.INF")
		}
	}

	const WORDS: [&str; 6] = ["Hello", "world", ".", "Next", "line", " "];

	fn text_of(text: &[u8]) -> Vec<String> {
		decode(&[Slot { text, words: WORDS.to_vec() }], &Book).iter().map(Block::text).collect()
	}

	fn runs_of(text: &[u8]) -> Vec<Run> {
		decode(&[Slot { text, words: WORDS.to_vec() }], &Book).into_iter().flat_map(|block| block.runs).collect()
	}

	fn link_of(escape: &[u8]) -> Option<Link> {
		let mut text = escape.to_vec();
		text.push(0);
		runs_of(&text).into_iter().find_map(|run| match run {
			Run::Text { link, .. } => link,
			_ => None,
		})
	}

	#[test]
	fn words_are_spaced_and_the_space_before_the_end_is_dropped() {
		assert_eq!(text_of(&[0, 1, PARAGRAPH, 3, 4]), ["Hello world", "Next line"]);
	}

	#[test]
	fn the_toggle_abuts_the_words_after_it_until_it_is_flipped_back() {
		// IPFC writes `CONFIG.SYS.` as the word before, a toggle, `CONFIG . SYS`, a toggle and `.`.
		assert_eq!(text_of(&[0, TOGGLE_SPACING, 1, 2, 1, TOGGLE_SPACING, 2, 3]), ["Hello world.world. Next"]);
	}

	#[test]
	fn a_line_break_ends_the_line_and_turns_spacing_back_on() {
		assert_eq!(text_of(&[TOGGLE_SPACING, 0, LINE_BREAK, 3, 4]), ["Hello\nNext line"]);
	}

	#[test]
	fn an_example_keeps_its_spaces_and_lines() {
		let text = [ESCAPE, 2, 0x0b, SPACE, SPACE, 0, SPACE, 1, LINE_BREAK, 3, ESCAPE, 2, 0x0c, 4];
		let blocks = decode(&[Slot { text: &text, words: WORDS.to_vec() }], &Book);
		assert_eq!(blocks[0].kind, BlockKind::Example);
		assert_eq!(blocks[0].text(), "  Hello world\nNext");
		assert_eq!(blocks[1].text(), "line");
	}

	#[test]
	fn a_slot_spaced_by_hand_is_read_with_spacing_off() {
		assert_eq!(
			text_of(&[0, SPACE, 1, 2, SPACE, 3, SPACE, 4, PARAGRAPH, 3, SPACE, 4]),
			["Hello world. Next line", "Next line"]
		);
	}

	#[test]
	fn styles_split_runs() {
		let runs = runs_of(&[0, ESCAPE, 3, 0x04, 2, 1, ESCAPE, 3, 0x04, 0, 3]);
		let styles: Vec<(String, bool)> = runs
			.into_iter()
			.filter_map(|run| match run {
				Run::Text { text, style, .. } => Some((text, style.bold)),
				_ => None,
			})
			.collect();
		assert_eq!(styles, [("Hello ".into(), false), ("world ".into(), true), ("Next".into(), false)]);
		assert_eq!(style(2), Style { bold: true, italic: false, underline: false });
	}

	#[test]
	fn links_carry_their_targets() {
		assert_eq!(link_of(&[ESCAPE, 4, 0x05, 7, 0]), Some(Link::Panel(7)));
		assert_eq!(link_of(&[ESCAPE, 4, 0x07, 9, 0]), Some(Link::Footnote(9)));
		assert_eq!(link_of(&[ESCAPE, 4, 0x16, 0x88, 0x13]), Some(Link::Inform(5000)));
		let launch = [&[ESCAPE, 11, 0x10, 11][..], b"view.exe"].concat();
		assert_eq!(link_of(&launch), Some(Link::Launch("view.exe".into())));
		let other = [&[ESCAPE, 8, 0x1f, 0, 4][..], b"BM64"].concat();
		assert_eq!(link_of(&other), Some(Link::OtherFile { file: Some("OTHER.INF".into()), panel: "BM64".into() }));
	}

	#[test]
	fn a_child_window_puts_nothing_in_the_text() {
		assert_eq!(text_of(&[0, ESCAPE, 7, 0x0f, 0x0b, 1, 0, 1, 0, ESCAPE, 5, 0x0f, 4, 1, 0, 1]), ["Hello world"]);
	}

	#[test]
	fn hidden_text_is_left_out() {
		assert_eq!(text_of(&[0, ESCAPE, 2, 0x17, 1, ESCAPE, 2, 0x18, 3]), ["Hello Next"]);
	}

	#[test]
	fn a_damaged_slot_stops_rather_than_panicking() {
		assert_eq!(text_of(&[0, 200, ESCAPE, 40, 0x05]), ["Hello"]);
	}
}
