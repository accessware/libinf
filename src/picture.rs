//! Pictures, which IPFC stores as OS/2 bitmaps in the file's image area.
//!
//! A picture starts with an OS/2 1.x bitmap header and colour table. Then comes its pixel data, compressed one of several ways:
//!
//! - `bM`, written by later compilers: blocks, each stored, LZW compressed or run-length encoded.
//! - `BM`, written by earlier ones: stored; run-length encoded as count and value pairs; or cut into strips of rows, each stored or run-length encoded.
//! - `BA`, a bitmap array: a short header, then one of the above.
//!
//! None of this is documented. The LZW scheme follows the EDM2 article "Inside INF"; the rest was worked out from real files.

use crate::{
	InfError, Result,
	bytes::{slice, u16_at, u32_at},
};

/// The largest picture this crate decodes, in bytes of pixel data. The biggest in real files is under 2 MB, and anything larger is a damaged header.
const MAX_PIXEL_BYTES: usize = 64 << 20;

/// A decoded picture, laid out as a Windows bitmap's pixel data so it converts to one with [`Picture::to_bmp`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
	/// The width in pixels.
	pub width: u32,
	/// The height in pixels.
	pub height: u32,
	/// Bits per pixel: 1, 4, 8 or 24.
	pub bits_per_pixel: u16,
	/// The colours a pixel's value picks, as red, green and blue. Empty for a 24-bit picture, whose pixels are colours.
	pub palette: Vec<[u8; 3]>,
	/// The rows from the bottom up, each padded to a multiple of four bytes, as in a bitmap file. A 24-bit pixel is blue, green and red.
	pub pixels: Vec<u8>,
}

impl Picture {
	/// The picture as a Windows bitmap file, which image libraries and viewers open.
	#[must_use]
	pub fn to_bmp(&self) -> Vec<u8> {
		const HEADERS: u32 = 14 + 40;
		let palette_len = u32::try_from(self.palette.len() * 4).unwrap_or(u32::MAX);
		let pixels_len = u32::try_from(self.pixels.len()).unwrap_or(u32::MAX);
		let data_start = HEADERS + palette_len;
		let mut bmp = Vec::with_capacity(self.pixels.len() + usize::try_from(data_start).unwrap_or(0));
		bmp.extend_from_slice(b"BM");
		bmp.extend_from_slice(&data_start.saturating_add(pixels_len).to_le_bytes());
		bmp.extend_from_slice(&[0; 4]);
		bmp.extend_from_slice(&data_start.to_le_bytes());
		bmp.extend_from_slice(&40_u32.to_le_bytes());
		bmp.extend_from_slice(&self.width.to_le_bytes());
		bmp.extend_from_slice(&self.height.to_le_bytes());
		bmp.extend_from_slice(&1_u16.to_le_bytes());
		bmp.extend_from_slice(&self.bits_per_pixel.to_le_bytes());
		bmp.extend_from_slice(&[0; 4]);
		bmp.extend_from_slice(&pixels_len.to_le_bytes());
		bmp.extend_from_slice(&[0; 16]);
		for [red, green, blue] in &self.palette {
			bmp.extend_from_slice(&[*blue, *green, *red, 0]);
		}
		bmp.extend_from_slice(&self.pixels);
		bmp
	}
}

/// Reads the picture at `at`, an offset into `data`.
pub fn read(data: &[u8], at: usize) -> Result<Picture> {
	let mut at = at;
	// A bitmap array holds versions of a picture for different displays; IPFC writes one.
	if slice(data, at, 2, "a picture")? == b"BA" {
		at += 14;
	}
	let magic = slice(data, at, 2, "a picture")?;
	let packed = match magic {
		b"bM" => true,
		b"BM" => false,
		_ => return Err(InfError::UnsupportedPicture("not a bitmap")),
	};
	if u32_at(data, at + 14, "a picture")? != 12 {
		return Err(InfError::UnsupportedPicture("an OS/2 2.0 bitmap header"));
	}
	let width = u16_at(data, at + 18, "a picture")?;
	let height = u16_at(data, at + 20, "a picture")?;
	let bits_per_pixel = u16_at(data, at + 24, "a picture")?;
	if !matches!(bits_per_pixel, 1 | 4 | 8 | 24) {
		return Err(InfError::UnsupportedPicture("an unusual number of bits per pixel"));
	}
	let row = (usize::from(width) * usize::from(bits_per_pixel)).div_ceil(32) * 4;
	let size = row * usize::from(height);
	if size > MAX_PIXEL_BYTES {
		return Err(InfError::UnsupportedPicture("too large"));
	}
	let colours = if bits_per_pixel <= 8 { 1 << bits_per_pixel } else { 0 };
	let palette = slice(data, at + 26, colours * 3, "a picture's colours")?
		.as_chunks::<3>()
		.0
		.iter()
		.map(|&[blue, green, red]| [red, green, blue])
		.collect();
	let start = at + 26 + colours * 3;
	let len = usize::try_from(u32_at(data, start, "a picture")?).unwrap_or(usize::MAX);
	let body = slice(data, start + 4, len, "a picture")?;
	let mut pixels = Pixels::new(size, row);
	if packed {
		read_blocks(body, &mut pixels)?;
	} else if len == size {
		pixels.extend(body);
	} else if !read_strips(body, &mut pixels, usize::from(height), bits_per_pixel == 1) {
		pixels = Pixels::new(size, row);
		read_pairs(body, &mut pixels);
	}
	let mut pixels = pixels.bytes;
	// A damaged or cut short picture still shows what it has.
	pixels.resize(size, 0);
	Ok(Picture { width: u32::from(width), height: u32::from(height), bits_per_pixel, palette, pixels })
}

/// The pixel data being decoded, which never grows past the picture's size, whatever a damaged file claims.
struct Pixels {
	bytes: Vec<u8>,
	limit: usize,
	row: usize,
}

impl Pixels {
	fn new(limit: usize, row: usize) -> Self {
		Self { bytes: Vec::with_capacity(limit), limit, row }
	}

	const fn room(&self) -> usize {
		self.limit - self.bytes.len()
	}

	fn extend(&mut self, bytes: &[u8]) {
		let take = bytes.len().min(self.room());
		self.bytes.extend_from_slice(&bytes[..take]);
	}

	fn repeat(&mut self, byte: u8, count: usize) {
		let take = count.min(self.room());
		self.bytes.resize(self.bytes.len() + take, byte);
	}
}

/// `bM` data: blocks of at most 64 KB once decoded, each with its length (counting the type byte) and how it is stored.
fn read_blocks(body: &[u8], pixels: &mut Pixels) -> Result<()> {
	// The first two bytes give how much each block decodes to, which the blocks themselves already show.
	let mut at = 2;
	while at < body.len() {
		let len = usize::from(u16_at(body, at, "a picture block")?);
		let block = slice(body, at + 2, len, "a picture block")?;
		at += 2 + len;
		let Some((&kind, block)) = block.split_first() else { continue };
		match kind {
			0 => pixels.extend(block),
			1 => unpack_runs(block, pixels),
			2 => unpack_lzw(block, pixels),
			_ => return Err(InfError::UnsupportedPicture("an unknown compression")),
		}
	}
	Ok(())
}

/// Run-length encoding in `bM` blocks. A byte below 0x80 repeats the next byte one more time than it says; above 0x80, that many less 0x7f bytes follow as they are. 0x80 starts a longer code: after 0 or 1 comes a 16-bit count of zero or 0xff bytes, again one more, and after anything else that many plus one copies of the two bytes that follow, for dithered areas.
fn unpack_runs(block: &[u8], pixels: &mut Pixels) {
	let mut at = 0;
	while let Some(&code) = block.get(at) {
		at += 1;
		match code {
			0..0x80 => {
				let Some(&byte) = block.get(at) else { return };
				pixels.repeat(byte, usize::from(code) + 1);
				at += 1;
			}
			0x80 => {
				let Some(&[kind, low, high]) = block.get(at..at + 3).and_then(|bytes| <&[u8; 3]>::try_from(bytes).ok())
				else {
					return;
				};
				at += 3;
				match kind {
					0 | 1 => pixels
						.repeat(if kind == 0 { 0 } else { 0xff }, usize::from(u16::from_le_bytes([low, high])) + 1),
					_ => {
						for _ in 0..=kind {
							pixels.extend(&[low, high]);
						}
					}
				}
			}
			_ => {
				let len = usize::from(code - 0x7f);
				let Some(bytes) = block.get(at..at + len) else { return };
				pixels.extend(bytes);
				at += len;
			}
		}
	}
}

/// LZW in `bM` blocks: codes of 9 to 12 bits, high bit first, widening one code early. Code 256 clears the table, 257 ends the block, and new strings start at 258.
fn unpack_lzw(block: &[u8], pixels: &mut Pixels) {
	const CLEAR: u16 = 256;
	const END: u16 = 257;
	const FIRST: u16 = 258;
	const MAX_WIDTH: u32 = 12;
	// Each code past 255 is an earlier code plus one byte.
	let mut prefix = [0_u16; 4096];
	let mut suffix = [0_u8; 4096];
	let mut string = Vec::with_capacity(4096);
	let mut next = FIRST;
	let mut width = 9;
	let mut previous: Option<u16> = None;
	let (mut buffer, mut buffered, mut at) = (0_u32, 0_u32, 0);
	loop {
		while buffered < width {
			let Some(&byte) = block.get(at) else { return };
			buffer = (buffer << 8) | u32::from(byte);
			buffered += 8;
			at += 1;
		}
		buffered -= width;
		let code = u16::try_from((buffer >> buffered) & ((1 << width) - 1)).unwrap_or(END);
		buffer &= (1 << buffered) - 1;
		match code {
			CLEAR => {
				next = FIRST;
				width = 9;
				previous = None;
				continue;
			}
			END => return,
			_ => {}
		}
		let Some(last) = previous else {
			if code > 0xff {
				return;
			}
			pixels.extend(&[u8::try_from(code).unwrap_or(0)]);
			previous = Some(code);
			continue;
		};
		// A code not yet in the table can only be the one about to be added: the previous string plus its own first byte.
		let known = code < next;
		if !known && code != next {
			return;
		}
		string.clear();
		let mut walk = if known { code } else { last };
		while walk > 0xff {
			string.push(suffix[usize::from(walk)]);
			walk = prefix[usize::from(walk)];
		}
		let first = u8::try_from(walk).unwrap_or(0);
		string.push(first);
		string.reverse();
		if !known {
			string.push(first);
		}
		pixels.extend(&string);
		if usize::from(next) < prefix.len() {
			prefix[usize::from(next)] = last;
			suffix[usize::from(next)] = first;
			next += 1;
		}
		if u32::from(next) + 1 >= 1 << width && width < MAX_WIDTH {
			width += 1;
		}
		previous = Some(code);
	}
}

/// `BM` data cut into strips: the number of rows in each, then each strip's length and data, stored as it is when the length is zero. Returns false if `body` is not laid out that way, since nothing in the header says.
fn read_strips(body: &[u8], pixels: &mut Pixels, height: usize, bit_runs: bool) -> bool {
	let Ok(rows) = u16_at(body, 0, "a picture") else { return false };
	let rows = usize::from(rows);
	if rows == 0 {
		return false;
	}
	let strip_size = rows * pixels.row;
	let mut at = 2;
	for _ in 0..height.div_ceil(rows) {
		let Ok(len) = u16_at(body, at, "a picture") else { return false };
		at += 2;
		let (len, stored) = if len == 0 { (strip_size, true) } else { (usize::from(len), false) };
		let Some(strip) = body.get(at..at + len) else { return false };
		at += len;
		let before = pixels.bytes.len();
		if stored {
			pixels.extend(strip);
		} else if bit_runs {
			unpack_bit_runs(strip, pixels);
		} else {
			read_pairs(strip, pixels);
		}
		// Every strip holds its full number of rows, the last one too, so anything else means the guess was wrong.
		if pixels.bytes.len() - before != strip_size.min(pixels.limit - before) {
			return false;
		}
	}
	at == body.len()
}

/// Count and value pairs, each repeating the value as many times as the count says.
fn read_pairs(body: &[u8], pixels: &mut Pixels) {
	for &[count, value] in body.as_chunks::<2>().0 {
		pixels.repeat(value, usize::from(count));
	}
}

/// A one-bit picture's strips as runs of pixels: the low seven bits of each byte count them, and a clear top bit makes them ones.
fn unpack_bit_runs(strip: &[u8], pixels: &mut Pixels) {
	let (mut byte, mut filled) = (0_u8, 0);
	for &run in strip {
		let bit = u8::from(run & 0x80 == 0);
		for _ in 0..run & 0x7f {
			byte = byte << 1 | bit;
			filled += 1;
			if filled == 8 {
				pixels.extend(&[byte]);
				(byte, filled) = (0, 0);
			}
		}
	}
}
