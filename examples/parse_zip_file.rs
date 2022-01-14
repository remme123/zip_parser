use std::env;
use std::fs::File;

use zip_parser::*;

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        panic!("no zip file specified")
    }
    let file = File::open(args[1].as_str()).unwrap();
    for (i, file) in Parser::new(file).enumerate() {
        println!("{}: {:02X?}", i, file);
    }
}
