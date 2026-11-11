# libinf

Pure-Rust reader for OS/2 Information Presentation Facility help files: books (`.inf`) and application help (`.hlp`). No unsafe code, no C dependencies.

## Usage

```rust
use libinf::InfFile;

fn main() -> libinf::Result<()> {
    let book = InfFile::open("cmdref.inf")?;
    println!("{}", book.title().unwrap_or("untitled"));
    for (index, entry) in book.toc().iter().enumerate() {
        println!("== {} ==", entry.title);
        println!("{}", book.panel(index)?.text());
    }
    Ok(())
}
```

There are worked examples in `examples/`: `dump` prints a file's contents and text, and `pictures` saves its pictures as bitmaps.

```sh
cargo run --example dump -- cmdref.inf
cargo run --example pictures -- cmdref.inf pictures/
```

## The format

IBM never documented the compiled format. This crate follows Carl Hauser and Marcus Gröber's reverse-engineered description, version 2B (1994), checked against real files.

Links say where they lead: another panel or a footnote by its place in the table of contents, a panel in another file by name, a program with its command line, or a message to the application. `InfFile::panel_by_name` and `InfFile::panel_by_resource` find the panels other files and applications link to.

Pictures are OS/2 bitmaps, compressed with LZW, with two kinds of run-length encoding, or not at all. The LZW scheme follows the EDM2 article "Inside INF"; the rest was worked out from real files. `InfFile::picture` decodes one, and `Picture::to_bmp` turns it into a Windows bitmap.

OS/2 `.hlp` files have nothing to do with WinHelp's, and `.inf` is also the extension of Windows setup files. All IPF files start with `HSP`; `libinf::is_ipf` checks for it.

## License

MIT
