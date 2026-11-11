//! Saves every picture a help file's panels place as a Windows bitmap.
//!
//! ```sh
//! cargo run --example pictures -- tutorial.inf pictures/
//! ```

use std::{collections::BTreeSet, env, fs, path::PathBuf, process::ExitCode};

use libinf::{ImageRef, InfFile, Run};

fn main() -> ExitCode {
	let args: Vec<String> = env::args().skip(1).collect();
	let [path, folder] = args.as_slice() else {
		eprintln!("usage: pictures <file.inf> <folder>");
		return ExitCode::FAILURE;
	};
	let book = match InfFile::open(path) {
		Ok(book) => book,
		Err(error) => {
			eprintln!("{path}: {error}");
			return ExitCode::FAILURE;
		}
	};
	let mut offsets = BTreeSet::new();
	for index in 0..book.toc().len() {
		let Ok(panel) = book.panel(index) else { continue };
		for block in &panel.blocks {
			for run in &block.runs {
				if let Run::Image(image) = run {
					offsets.insert(image.offset);
				}
			}
		}
	}
	let folder = PathBuf::from(folder);
	if let Err(error) = fs::create_dir_all(&folder) {
		eprintln!("{}: {error}", folder.display());
		return ExitCode::FAILURE;
	}
	let (mut saved, mut failed) = (0, 0);
	for offset in offsets {
		match book.picture(ImageRef { offset }) {
			Ok(picture) => {
				let file = folder.join(format!("{offset:08x}.bmp"));
				if let Err(error) = fs::write(&file, picture.to_bmp()) {
					eprintln!("{}: {error}", file.display());
					return ExitCode::FAILURE;
				}
				saved += 1;
			}
			Err(error) => {
				println!("{offset:08x}: {error}");
				failed += 1;
			}
		}
	}
	println!("{saved} saved, {failed} failed");
	ExitCode::SUCCESS
}
