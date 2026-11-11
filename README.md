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

There is a worked example in `examples/dump.rs`:

```sh
cargo run --example dump -- cmdref.inf
```

## The format

IBM never documented the compiled format. This crate follows Carl Hauser and Marcus Gröber's reverse-engineered description, version 2B (1994), checked against real files.

OS/2 `.hlp` files have nothing to do with WinHelp's, and `.inf` is also the extension of Windows setup files. All IPF files start with `HSP`; `libinf::is_ipf` checks for it.

## License

MIT
