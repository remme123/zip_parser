# zip_parser

Zip file format parser implemented by rust, supports stream parsing, `no_std` environment.

The [`Parser`] will search central directory at the end of zip file if [`Seek`] is available.
Also, It supports sequence read parsing when [`Seek`] is not available.
All types in std env implemented `std::io::Read` automatically implement [`Read`], and so is the trait [`Seek`].
[`PassiveParser`] can be used for some situations in which data is recvieved by other task.

### stream parsing
```rust
#[derive(Debug)]
struct DataBuffer {
    index: usize,
    pub buffer: Vec<u8>,
}

fn file_stream_parsing(mut file: File) {
    println!("*** get stream from file ***");
    let mut buffer = DataBuffer {
        index: 0,
        buffer: Vec::new(),
    };
    let file_size = file.read_to_end(&mut buffer.buffer).unwrap_or(0);
    println!("zip size: {} bytes", file_size);

    println!("SequentialParser:");
    parse(SequentialParser::<DataBuffer>::new(&mut buffer));

    println!("\n\nPassiveParser:");
    let mut parser = PassiveParser::<128>::new();
    parser.feed_data(
        &buffer.buffer,
        |evt| {
            match evt {
                ParserEvent::LocalFileHeader(file_index, file) => {
                    println!("#{}: {}({}/{} bytes)",
                    file_index, file.file_name().unwrap_or("Utf8EncodeErr"),
                        file.compressed_size, file.uncompressed_size);
                    true
                },
                ParserEvent::ParsingError(_file_index, e) => {
                    println!("error: {e}");
                    false
                },
                _ => {
                    // println!("unprocessed event: {evt:?}");
                    true
                },
            }
        }
    );
    println!(r#"zip file comment: "{}""#, parser.file_comment().unwrap())
}

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() >= 2 {
        let file = File::open(args[1].as_str()).unwrap();
        file_stream_parsing(file);
    }
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
2. Stream parsing (Read and PassiveParser) from file
    ```bash
    cargo run --features="std" --example stream_parsing -- test.zip
    ```
#### Passive parsing
In example [`stream_parsing`], there is a case for passive parsing:
read data from a file and [`PassiveParser::feed_data`] to the parser.
