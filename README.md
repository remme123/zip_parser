# zip_parser

Zip file format parser implemented by rust, supports stream parsing, `no_std` environment.

The [`Parser`] will search central directory at the end of zip file if [`Seek`] is available.
Also, It supports sequence read parsing when [`Seek`] is not available.
All types in std env implemented `std::io::Read` automatically implement [`Read`], and so is the trait [`Seek`].

### stream parsing
```rust
use zip_parser as zip;
use zip::prelude::*;

#[cfg(feature = "std")]
fn parse<S: zip::Read + zip::Seek>(parser: Parser<S>) {
    for (i, mut file) in parser.enumerate() {
        println!("{}: {}({} Bytes)", i, unsafe { file.file_name() }, file.file_size());
        let mut buf = Vec::new();
        buf.resize(file.file_size() as usize, 0);
        if let Ok(n) = file.read(&mut buf) {
            println!("Data: {:02X?}", &buf[..n]);
        } else {
            println!("read failed");
        }
        println!();
    }
}

#[cfg(feature = "std")]
fn stdin_parsing() {
    println!("*** get stream from stdin ***");
    parse(Parser::new(std::io::stdin().lock()))
}
```
You just need to pass a stream which implements [`Read`] into the [`Parser::new()`](struct.Parser.html#method.new),
then you can iterate over it. For more detail, see example `stream_parsing`.

### Example
#### Stream_parsing
1. From `stdin`
    ```bash
    cat test.zip | cargo run --features="std" --example stream_parsing
    ```
    or even you can cat multiple zip files:
    ```bash
    cat test.zip test.zip | cargo run --features="std" --example stream_parsing
    ```
1. From file
    ```bash
    cargo run --features="std" --example stream_parsing -- test.zip
    ```

License: MIT
# zip_parser

Zip file format parser implemented by rust, supports stream parsing, `no_std` environment.

The [`Parser`] will search central directory at the end of zip file if [`Seek`] is available.
Also, It supports sequence read parsing when [`Seek`] is not available.
All types in std env implemented `std::io::Read` automatically implement [`Read`], and so is the trait [`Seek`].

### stream parsing
```rust
use zip_parser as zip;
use zip::prelude::*;

#[cfg(feature = "std")]
fn parse<S: zip::Read + zip::Seek>(parser: Parser<S>) {
    for (i, mut file) in parser.enumerate() {
        println!("{}: {}({} Bytes)", i, unsafe { file.file_name() }, file.file_size());
        let mut buf = Vec::new();
        buf.resize(file.file_size() as usize, 0);
        if let Ok(n) = file.read(&mut buf) {
            println!("Data: {:02X?}", &buf[..n]);
        } else {
            println!("read failed");
        }
        println!();
    }
}

#[cfg(feature = "std")]
fn stdin_parsing() {
    println!("*** get stream from stdin ***");
    parse(Parser::new(std::io::stdin().lock()))
}
```
You just need to pass a stream which implements [`Read`] into the [`Parser::new()`](struct.Parser.html#method.new),
then you can iterate over it. For more detail, see example `stream_parsing`.

### Example
#### Stream_parsing
1. From `stdin`
    ```bash
    cat test.zip | cargo run --features="std" --example stream_parsing
    ```
    or even you can cat multiple zip files:
    ```bash
    cat test.zip test.zip | cargo run --features="std" --example stream_parsing
    ```
1. From file
    ```bash
    cargo run --features="std" --example stream_parsing -- test.zip
    ```

License: MIT
