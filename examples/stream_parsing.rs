use std::env;
use std::fs::File;
use std::io::Read;

use zip_parser::*;

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

impl zip_parser::Seek for DataBuffer {
}

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        panic!("no zip file specified")
    }
    let mut file = File::open(args[1].as_str()).unwrap();
    let mut buffer = DataBuffer {
        index: 0,
        buffer: Vec::new(),
    };
    let file_size = file.read_to_end(&mut buffer.buffer).unwrap_or(0);

    for (i, mut file) in Parser::new(buffer).enumerate() {
        println!("{}: {}({} Bytes): {:02X?}", i, unsafe { file.file_name() }, file.file_size(), file);
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
