use std::io;

use thiserror::Error;

/// What can go wrong reading a help file.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InfError {
	/// The file could not be read.
	#[error("could not read the file: {0}")]
	Io(#[from] io::Error),
	/// The file does not start with the `HSP` signature IPFC writes, so it is not an IPF help file. Windows setup files and WinHelp files share the `.inf` and `.hlp` extensions.
	#[error("not an OS/2 help file")]
	BadMagic,
	/// A structure the header points to runs past the end of the file.
	#[error("the file is truncated or damaged: {0} runs past its end")]
	Truncated(&'static str),
	/// A table of contents entry the caller asked for does not exist.
	#[error("there is no table of contents entry {0}")]
	NoSuchEntry(usize),
	/// A picture is stored in a way this crate can't decode, such as a metafile.
	#[error("the picture is {0}, which this crate can't decode")]
	UnsupportedPicture(&'static str),
}

/// The result of reading a help file.
pub type Result<T> = std::result::Result<T, InfError>;
