//! Prints a help file's table of contents and every panel's text.
//!
//! ```sh
//! cargo run --example dump -- cmdref.inf
//! ```

use std::{env, process::ExitCode};

use libinf::InfFile;

fn main() -> ExitCode {
	let Some(path) = env::args().nth(1) else {
		eprintln!("usage: dump <file.inf>");
		return ExitCode::FAILURE;
	};
	let book = match InfFile::open(&path) {
		Ok(book) => book,
		Err(error) => {
			eprintln!("{path}: {error}");
			return ExitCode::FAILURE;
		}
	};
	println!("title: {}", book.title().unwrap_or("(none)"));
	println!("kind: {:?}, code page: {:?}", book.kind(), book.code_page());
	for (index, entry) in book.toc().iter().enumerate() {
		let indent = "  ".repeat(usize::from(entry.level.saturating_sub(1)));
		let hidden = if entry.hidden { " (hidden)" } else { "" };
		println!("\n{indent}== {} =={hidden}", entry.title);
		match book.panel(index) {
			Ok(panel) => println!("{}", panel.text()),
			Err(error) => println!("[{error}]"),
		}
	}
	match book.index() {
		Ok(index) => println!("\nindex: {} entries", index.len()),
		Err(error) => println!("\nindex: {error}"),
	}
	ExitCode::SUCCESS
}
