use std::env;
use std::fs::File;
use std::io::{Read, stdin};
use std::cmp;

use zip_parser as zip;
use zip::prelude::*;

fn parse<'a, P>(parser: P)
where
    P: Iterator,
    P::Item: zip_parser::LocalFileOps
{
    for (i, mut file) in parser.enumerate() {
        println!("#{}: {}({} Bytes)", i, file.file_name().unwrap_or("NoFileName"), file.file_size());
        let mut buf = Vec::new();
        buf.resize(file.file_size() as usize, 0);
        if let Err(e) = file.read_exact(&mut buf) {
            println!("read failed: {}", e);
        } else {
            println!("Data: {:02X?}", &buf[..16]);
        }
        println!();
    }
}

use std::io::StdinLock;

fn stdin_parsing() {
    println!("*** get stream from stdin ***");
    parse(SequentialParser::<StdinLock<'_>>::new(&mut stdin().lock()))
}

#[derive(Debug)]
struct DataBuffer {
    index: usize,
    pub buffer: Vec<u8>,
}

impl zip_parser::Read for DataBuffer {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError> {
        // println!("read {}", buf.len());
        let len = cmp::min(
            self.buffer.len() - self.index,
            buf.len(),
        );
        // limit read size, only for live streaming parsing test
        // if len > 8 {
        //     len = 8;
        // }
        if len > 0 {
            buf[..len].copy_from_slice(&self.buffer[self.index..self.index + len]);
            self.index += len;
            // println!("return {}", len);
            Ok(len)
        } else {
            Err(ParsingError::StreamEnding)
        }
    }
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
    let mut parser = PassiveParser::<DataBuffer, 128>::new();
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
    if args.len() < 2 {
        stdin_parsing();
    } else {
        let file = File::open(args[1].as_str()).unwrap();
        file_stream_parsing(file);
    }
}
