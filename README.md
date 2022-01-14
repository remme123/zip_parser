# zip_parser
Zip file format parser implemented by rust. support stream parsing, no_std environment.

The `Parser` will search central directory at the end of zip file if `Seek` is available. Also, It supports sequence read parsing when `Seek` is not available. The type which implements `std::io::Read` implements `Read` in `std` env, and so is the `Seek`. 
