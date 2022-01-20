# zip_parser
Zip file format parser implemented by rust. support stream parsing, no_std environment.

The `Parser` will search central directory at the end of zip file if `Seek` is available. Also, It supports sequence read parsing when `Seek` is not available. The type which implements `std::io::Read` implements `Read` in `std` env, and so is the `Seek`. 

## stream parsing
```rust
fn parse<S: zip::Read + zip::Seek>(mut parser: Parser<S>) {
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

fn stdin_parsing() {
    println!("*** get stream from stdin ***");
    parse(Parser::new(stdin().lock()))
}
```
You just need to pass a stream which implements `Read` into the `Parser::new()`, then you can iterate over it. For more detail, see example `stream_parsing`