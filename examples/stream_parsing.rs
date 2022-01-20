use std::env;
use std::fs::File;
use std::io::{Read, stdin};

use zip_parser as zip;
use zip::*;

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

#[derive(Debug)]
struct DataBuffer {
    index: usize,
    pub buffer: Vec<u8>,
}

impl zip_parser::Read for DataBuffer {
    fn read(&mut self, buf: &mut [u8]) -> zip_parser::ReadResult {
        println!("read {}", buf.len());
        let len = if self.buffer.len() - self.index < buf.len() {
            self.buffer.len() - self.index
        } else {
            buf.len()
        };
        if len > 0 {
            buf.copy_from_slice(&self.buffer[self.index..self.index + len]);
            self.index += len;
            Ok(len)
        } else {
            Ok(0)
        }
    }
}

fn file_parsing() {
    println!("*** get stream from file ***");
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        panic!("specify zip file path")
    }
    let mut file = File::open(args[1].as_str()).unwrap();
    let mut buffer = DataBuffer {
        index: 0,
        buffer: Vec::new(),
    };
    let file_size = file.read_to_end(&mut buffer.buffer).unwrap_or(0);

    parse(Parser::new(buffer))
}

fn main() {
    stdin_parsing();
}
