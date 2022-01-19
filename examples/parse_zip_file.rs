use std::env;
use std::fs::File;

use zip_parser::*;

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        panic!("no zip file specified")
    }
    let file = File::open(args[1].as_str()).unwrap();
    for (i, mut file) in Parser::new(file).enumerate() {
        println!("{}: {}({} Bytes): {:02X?}", i, unsafe { file.file_name() }, file.file_size(), file);
        let mut buf = [0u8; 16];
        if let Ok(n) = file.read(&mut buf) {
            println!("Data: {:02X?}", &buf[..n]);
        } else {
            println!("read failed");
        }
    }
}
