use std::env;
use std::fs::File;
use std::io::{Read, stdin};

use zip_parser as zip;
use zip::prelude::*;

fn parse<'a, S: zip::Read, P: zip::Parser<S>>(parser: P) where
    <P as Iterator>::Item: zip_parser::LocalFileOps {
    for (i, mut file) in parser.enumerate() {
        println!("{}: {}({} Bytes)", i, file.file_name().unwrap_or("NoFileName"), file.file_size());
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

fn stdin_parsing() {
    println!("*** get stream from stdin ***");
    parse(SequentialParser::new(stdin().lock()))
}

#[derive(Debug)]
struct DataBuffer {
    index: usize,
    pub buffer: Vec<u8>,
}

impl zip_parser::Read for DataBuffer {
    fn read(&mut self, buf: &mut [u8]) -> zip_parser::ReadResult {
        // println!("read {}", buf.len());
        let len = if self.buffer.len() - self.index < buf.len() {
            self.buffer.len() - self.index
        } else {
            buf.len()
        };
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
            Ok(0)
        }
    }
}

fn file_parsing(mut file: File) {
    println!("*** get stream from file ***");
    let mut buffer = DataBuffer {
        index: 0,
        buffer: Vec::new(),
    };
    let _file_size = file.read_to_end(&mut buffer.buffer).unwrap_or(0);

    parse(SequentialParser::new(buffer))
}

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        stdin_parsing();
    } else {
        let file = File::open(args[1].as_str()).unwrap();
        file_parsing(file);
    }
}
