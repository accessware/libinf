//! Bounds-checked little-endian reads, since every offset in a help file is untrusted.

use crate::{InfError, Result};

pub fn u8_at(data: &[u8], offset: usize, what: &'static str) -> Result<u8> {
	data.get(offset).copied().ok_or(InfError::Truncated(what))
}

pub fn u16_at(data: &[u8], offset: usize, what: &'static str) -> Result<u16> {
	let bytes = slice(data, offset, 2, what)?;
	Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

pub fn u32_at(data: &[u8], offset: usize, what: &'static str) -> Result<u32> {
	let bytes = slice(data, offset, 4, what)?;
	Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub fn slice<'a>(data: &'a [u8], offset: usize, len: usize, what: &'static str) -> Result<&'a [u8]> {
	offset.checked_add(len).and_then(|end| data.get(offset..end)).ok_or(InfError::Truncated(what))
}

/// A file offset read from the file, as an index.
pub fn offset(value: u32) -> usize {
	usize::try_from(value).unwrap_or(usize::MAX)
}
