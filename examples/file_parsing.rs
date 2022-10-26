use std::env;
use std::fs::File;

use zip_parser::prelude::*;

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        panic!("no zip file specified")
    }
    let mut file = File::open(args[1].as_str()).unwrap();
    for (i, mut file) in SeekingParser::<File, 128>::new(&mut file).enumerate() {
        println!("{}: {}({} Bytes)", i, file.file_name().unwrap_or("NoFileName"), file.file_size());
        let mut buf = [0u8; 16];
        if let Ok(n) = file.read(&mut buf) {
            println!("Data: {:02X?}", &buf[..n]);
        } else {
            println!("read failed");
        }
        println!();
    }
}
