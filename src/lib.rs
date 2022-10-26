//! Zip file format parser implemented by rust, supports stream parsing, `no_std` environment.
//!
//! The [`Parser`] will search central directory at the end of zip file if [`Seek`] is available.
//! Also, It supports sequence read parsing when [`Seek`] is not available.
//! All types in std env implemented `std::io::Read` automatically implement [`Read`], and so is the trait [`Seek`].
//!
//! ## stream parsing
//! ```
//! use zip_parser as zip;
//! use zip::prelude::*;
//!
//! #[cfg(feature = "std")]
//! fn parse<S: zip::Read + zip::Seek>(parser: Parser<S>) {
//!     for (i, mut file) in parser.enumerate() {
//!         println!("{}: {}({} Bytes)", i, unsafe { file.file_name() }, file.file_size());
//!         let mut buf = Vec::new();
//!         buf.resize(file.file_size() as usize, 0);
//!         if let Ok(n) = file.read(&mut buf) {
//!             println!("Data: {:02X?}", &buf[..n]);
//!         } else {
//!             println!("read failed");
//!         }
//!         println!();
//!     }
//! }
//!
//! #[cfg(feature = "std")]
//! fn stdin_parsing() {
//!     println!("*** get stream from stdin ***");
//!     parse(Parser::new(std::io::stdin().lock()))
//! }
//! ```
//! You just need to pass a stream which implements [`Read`] into the [`Parser::new()`](struct.Parser.html#method.new),
//! then you can iterate over it. For more detail, see example `stream_parsing`.
//!
//! ## Example
//! ### Stream_parsing
//! 1. From `stdin`
//!     ```bash
//!     cat test.zip | cargo run --features="std" --example stream_parsing
//!     ```
//!     or even you can cat multiple zip files:
//!     ```bash
//!     cat test.zip test.zip | cargo run --features="std" --example stream_parsing
//!     ```
//! 1. From file
//!     ```bash
//!     cargo run --features="std" --example stream_parsing -- test.zip
//! ### Passive parsing
//! In example [`stream_parsing`], there is a case for passive parsing:
//! read data from a file and [`PassiveParser::feed_data`] to the parser.
//!

#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

use core::fmt::Display;
use core::{
    str, mem, ptr, slice, cmp,
};
use core::marker::PhantomData;
use core::convert::{TryFrom};
use core::str::Utf8Error;

#[cfg(feature = "std")]
use std::{io, vec::Vec};

/// Pure LocalFile header len, not include filename & extra field
pub const LOCAL_FILE_HEADER_LEN: usize = mem::size_of::<LocalFileHeader>();
pub const CENTRAL_FILE_HEADER_LEN: usize = mem::size_of::<CentralFileHeader>();
pub const CENTRAL_DIR_END_LEN: usize = mem::size_of::<CentralDirEnd>();

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError>;

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError> {
        let mut i = 0;
        while i < buf.len() {
            match self.read(&mut buf[i..]) {
                Ok(n) => {
                    i += n;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(0)
    }
}

#[cfg(feature = "std")]
impl<T> Read for T where T: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError> {
        self.read(buf).or(Err(ParsingError::Generic))
    }
}

pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

#[cfg(feature = "std")]
impl Into<io::SeekFrom> for SeekFrom {
    fn into(self) -> io::SeekFrom {
        match self {
            SeekFrom::Start(n) => io::SeekFrom::Start(n),
            SeekFrom::Current(n) => io::SeekFrom::Current(n),
            SeekFrom::End(n) => io::SeekFrom::End(n),
        }
    }
}

pub trait Seek {
    fn seek(&mut self, _pos: SeekFrom) -> Result<u64, &str> {
        Err("unimplemented")
    }

    fn rewind(&mut self) -> Result<(), &str> {
        if self.seek(SeekFrom::Start(0)).is_ok() {
            Ok(())
        } else {
            Err("seek to the beginning failed")
        }
    }

    fn stream_len(&mut self) -> Option<u64> {
        None
    }
}

#[cfg(feature = "std")]
impl<T: io::Seek> Seek for T
{
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, &str> {
        self.seek(pos.into()).or(Err("std.io.seek err"))
    }

    fn stream_len(&mut self) -> Option<u64> {
        let cur = self.stream_position().unwrap();
        let size = self.seek(io::SeekFrom::End(0)).unwrap();
        let _ = self.seek(io::SeekFrom::Start(cur));
        Some(size)
    }
}

#[repr(u32)]
#[derive(Debug, Copy, Clone)]
enum Signature {
    LocalFileHeader = 0x04034b50,
    CentralFileHeader = 0x02014b50,
    CentralDirEnd = 0x06054b50,
}

impl TryFrom<u32> for Signature {
    type Error = ParsingError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x04034b50 => Ok(Signature::LocalFileHeader),
            0x02014b50 => Ok(Signature::CentralFileHeader),
            0x06054b50 => Ok(Signature::CentralDirEnd),
            _ => Err(ParsingError::InvalidSignature),
        }
    }
}

impl TryFrom<[u8; 4]> for Signature {
    type Error = ParsingError;

    fn try_from(value: [u8; 4]) -> Result<Self, Self::Error> {
        u32::from_le_bytes([value[0], value[1], value[2], value[3]]).try_into()
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = ParsingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < 4 {
            return Err(ParsingError::DataNotEnough);
        }
        [value[0], value[1], value[2], value[3]].try_into()
    }
}

#[repr(packed)]
#[derive(Debug, Copy, Clone)]
struct LocalFileHeader {
    signature: Signature,
    version_needed_to_extract: u16,
    general_purpose_bit_flag: u16,
    compression_method: u16,
    last_mod_file_time: u16,
    last_mod_file_date: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    file_name_length: u16,
    extra_field_length: u16,
}

impl LocalFileHeader {
    pub fn len(&self) -> usize {
        mem::size_of::<Self>() + self.file_name_length as usize + self.extra_field_length as usize
    }

    pub unsafe fn from_bytes(ptr: &[u8]) -> Option<&Self> {
        (ptr.as_ptr() as *const Self).as_ref().and_then(|h| {
            if matches!(h.signature, Signature::LocalFileHeader) {
                Some(h)
            } else {
                None
            }
        })
    }

    pub unsafe fn get_extra_field(&self) -> &[u8] {
        let base = self as *const Self as *const u8;
        slice::from_raw_parts(
            base.offset(mem::size_of::<Self>() as isize)
                .offset(self.file_name_length as isize),
            self.extra_field_length as usize,
        )
    }
}

#[repr(packed)]
#[derive(Debug, Copy, Clone)]
struct CentralFileHeader {
    signature: Signature,
    version_made_by: u16,
    version_needed_to_extract: u16,
    general_purpose_bit_flag: u16,
    compression_method: u16,
    last_mod_file_time: u16,
    last_mod_file_date: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    file_name_length: u16,
    extra_field_length: u16,
    file_comment_length: u16,
    disk_number_start: u16,
    internal_file_attributes: u16,
    external_file_attributes: u32,
    relative_offset_of_local_header: u32,
}

impl CentralFileHeader {
    pub fn len(&self) -> usize {
        mem::size_of::<Self>()
            + self.file_name_length as usize
            + self.extra_field_length as usize
            + self.file_comment_length as usize
    }

    pub fn local_header_len(&self) -> usize {
        mem::size_of::<LocalFileHeader>()
            + self.file_name_length as usize
            + self.extra_field_length as usize
    }

    pub unsafe fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        (bytes.as_ptr() as *const Self).as_ref().and_then(|h| {
            if matches!(h.signature, Signature::CentralFileHeader) {
                Some(h)
            } else {
                None
            }
        })
    }

    pub unsafe fn get_extra_field(&self) -> &[u8] {
        let base = self as *const Self as *const u8;
        slice::from_raw_parts(
            base.offset((mem::size_of::<Self>() + self.file_name_length as usize) as isize),
            self.extra_field_length as usize,
        )
    }

    pub unsafe fn get_file_comment<'a>(&self) -> Result<&'a str, Utf8Error> {
        let base: *const u8 = self as *const Self as _;
        let b = slice::from_raw_parts(
            base.add(mem::size_of::<Self>() + self.file_name_length as usize + self.extra_field_length as usize),
            self.file_comment_length as usize,
        );
        core::str::from_utf8(b)
    }
}

#[repr(packed)]
#[derive(Debug, Copy, Clone)]
struct CentralDirEnd {
    signature: Signature,
    number_of_disk: u16,
    number_of_start_central_directory_disk: u16,
    total_entries_this_disk: u16,
    total_entries_all_disk: u16,
    size_of_the_central_directory: u32,
    central_directory_offset: u32,
    zip_file_comment_length: u16,
}

impl CentralDirEnd {
    pub fn len(&self) -> usize {
        mem::size_of::<Self>() + self.zip_file_comment_length as usize
    }

    pub unsafe fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        (bytes.as_ptr() as *const Self).as_ref().and_then(|h| {
            if matches!(h.signature, Signature::CentralDirEnd) {
                Some(h)
            } else {
                None
            }
        })
    }
}

pub trait LocalFileOps {
    fn file_name(&self) -> Result<&str, Utf8Error>;

    fn file_size(&self) -> u64;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError>;

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError>;
}

/// Parser event for callback
#[derive(Debug, Clone, Copy)]
pub enum ParserEvent<'a, 'b, 'c, S, const N: usize>
where
    S: Read + 'a,
    'a: 'c,
{
    LocalFileHeader(i32, &'c LocalFile<'a, S, N>),
    LocalFileData{file_index: i32, offset: usize, data: &'b [u8]},
    LocalFileEnd(i32),

    ParsingError(i32, ParsingError),

    /// Pattern: (local_file_index, consumed_bytes)
    UserCancel(i32, usize),
}

#[derive(Debug, Clone, Copy)]
pub enum ParsingError {
    /// Pattern: (local_file_index, filename_len)
    LocalFileNameTooLong(i32, usize),

    /// Pattern: (local_file_index)
    InvalidLocalFileHeader,

    InvalidCentralFileHeader,

    InvalidCentralDirEnd,

    /// Pattern: (local_file_index)
    LocalFileHeaderNotRecved(i32),

    Generic,

    InvalidStream,

    /// Reatch the end of stream
    StreamEnding,

    /// Invalid header signature
    InvalidSignature,

    DataNotEnough,
}

impl Display for ParsingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::LocalFileNameTooLong(i, n) => write!(f, "LocalFile #{}: LocalFileNameTooLong({})", i, n),
            Self::InvalidLocalFileHeader => write!(f, "InvalidLocalFileHeader"),
            Self::InvalidCentralFileHeader => write!(f, "InvalidCentralFileHeader"),
            Self::InvalidCentralDirEnd => write!(f, "InvalidCentralDirEnd"),
            Self::LocalFileHeaderNotRecved(i) => write!(f, "LocalFile #{}: LocalFileHeaderNotRecved", i),
            Self::Generic => write!(f, "GenericError"),
            Self::StreamEnding => write!(f, "StreamEnding"),
            Self::InvalidStream => write!(f, "InvalidStream"),
            Self::InvalidSignature => write!(f, "InvalidSignature"),
            Self::DataNotEnough => write!(f, "DataNotEnough"),
        }
    }
}

#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub enum CompressMethod {
    Uncompress = 0,
    Shrunk = 1,

    Reduced1 = 2,
    Reduced2 = 3,
    Reduced3 = 4,
    Reduced4 = 5,

    Imploded = 6,
    Deflated = 8,
    BZIP2 = 12,
    LZMA = 14,

    // IBM LZ77 z Architecture
    LZ77z = 19,

    /// Zstandard (zstd) Compression
    Zstd = 93,

    /// MP3 Compression
    MP3 = 94,

    /// XZ Compression
    XZ = 95,

    /// JPEG variant
    JPEG = 96,

    Unknown = 0xFFFF,
}

impl From<u16> for CompressMethod {
    fn from(value: u16) -> Self {
        match value {
            0 => Self::Uncompress,
            1 => Self::Shrunk,
            2 => Self::Reduced1,
            3 => Self::Reduced2,
            4 => Self::Reduced3,
            5 => Self::Reduced4,
            6 => Self::Imploded,
            8 => Self::Deflated,
            12 => Self::BZIP2,
            14 => Self::LZMA,
            19 => Self::LZ77z,
            93 => Self::Zstd,
            94 => Self::MP3,
            95 => Self::XZ,
            96 => Self::JPEG,
            _ => Self::Unknown,
        }
    }
}

/// File instance in the zip pack. You can get it by iterating over the [`Parser`].
#[derive(Debug)]
pub struct LocalFile<'a, S: Read, const N: usize> {
    file_name_buffer: [u8; N],
    file_name_length: usize,
    extra_field_length: usize,
    file_data_offset: u64,

    pub compression_method: CompressMethod,
    pub compressed_size: u64,
    pub uncompressed_size: u64,

    stream: *mut S,
    _marker: PhantomData<&'a mut S>,
}

impl<'a, S: Read, const N: usize> LocalFile<'a, S, N> {
    pub fn with_compression_method(mut self, method: CompressMethod) -> Self {
        self.compression_method = method;
        self
    }

    pub fn with_compressed_size(mut self, size: u64) -> Self {
        self.compressed_size = size;
        self
    }

    pub fn with_uncompressed_size(mut self, size: u64) -> Self {
        self.uncompressed_size = size;
        self
    }

    pub fn with_stream(mut self, stream: &mut S) -> Self {
        self.stream = stream;
        self
    }
}

impl<'a, S: Read, const N: usize> Default for LocalFile<'a, S, N> {
    fn default() -> Self {
        Self {
            file_name_buffer: [0; N],
            file_name_length: 0,
            extra_field_length: 0,
            file_data_offset: 0,
            compression_method: CompressMethod::Uncompress,
            compressed_size: 0,
            uncompressed_size: 0,
            stream: ptr::null_mut(),
            _marker:  PhantomData::default(),
        }
    }
}

impl<'a, S: Read, const N: usize> LocalFileOps for LocalFile<'a, S, N> {
    fn file_name(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(&self.file_name_buffer[..self.file_name_length])
    }

    fn file_size(&self) -> u64 {
        self.compressed_size
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError> {
        unsafe {
            self.stream
                .as_mut()
                .ok_or(ParsingError::InvalidStream)
                .and_then(|s| s.read(buf))
        }
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize, ParsingError> {
        unsafe {
            self.stream
                .as_mut()
                .ok_or(ParsingError::InvalidStream)
                .and_then(|s| s.read_exact(buf))
        }
    }
}

// #[cfg(feature = "std")]
// impl<'a, S: Read + Seek> io::Read for LocalFile<'a, S> {
//     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//         todo!()
//     }
// }

pub trait Parser<S: Read> {
    /// Creating an instance
    fn new(stream: &mut S) -> Self;
}

/// Zip file parser, creating it by [`new`](struct.Parser.html#method.new) method
pub struct SeekingParser<'a, S: Read + Seek, const N: usize = 128> {
    /// It will be None when no central directory was found
    pub number_of_files: Option<usize>,

    central_directory_offset: u64,
    /// offset relative to the central dir
    next_entry_offset: u64,

    /// holding the file handle
    stream: &'a mut S,
}

impl<'a, S: Read + Seek, const N: usize> SeekingParser<'a, S, N> {
    pub fn new(stream: &'a mut S) -> Self {
        // seek to the start of central directory
        let mut central_directory_offset = 0u64;
        let mut number_of_files = None;
        if let Some(stream_len) = stream.stream_len() {
            const READ_LEN: usize = mem::size_of::<CentralDirEnd>();
            if let Ok(_) = stream.seek(SeekFrom::Start(stream_len - READ_LEN as u64)) {
                let mut buf = [0u8; READ_LEN];
                if matches!(stream.read(&mut buf), Ok(n) if n == buf.len()) {
                    if matches!(Signature::try_from(buf.as_slice()), Ok(Signature::CentralDirEnd)) {
                        let central_dir = unsafe { CentralDirEnd::from_bytes(&buf).unwrap() };
                        let _ = stream.seek(SeekFrom::Start(central_dir.central_directory_offset as u64));
                        central_directory_offset = central_dir.central_directory_offset.into();
                        number_of_files = Some(central_dir.total_entries_this_disk.into());
                    } else {
                        let _ = stream.rewind();
                    }
                } else {
                    let _ = stream.rewind();
                }
            } else {
                #[cfg(feature = "std")]
                eprintln!("seek is unavailable, use SequentialParser instead");
            }
        } else {
            #[cfg(feature = "std")]
            eprintln!("seek is unavailable, use SequentialParser instead");
        }

        Self {
            stream,
            central_directory_offset,
            next_entry_offset: 0,
            number_of_files,
        }
    }
}

impl<'a, S: Read + Seek, const N: usize> Iterator for SeekingParser<'a, S, N> {
    type Item = LocalFile<'a, S, N>;

    fn next(&mut self) -> Option<Self::Item> {
        // seek read
        let _ = self.stream.seek(
            SeekFrom::Start(self.central_directory_offset + self.next_entry_offset)
        );
        let mut buf = [0u8; mem::size_of::<CentralFileHeader>()];
        match self.stream.read(&mut buf) {
            Ok(n) if n == buf.len() => {
                if let Some(file_info) = unsafe { CentralFileHeader::from_bytes(&buf) } {
                    // #[cfg(feature = "std")]
                    // dbg!(file_info);
                    let mut file = LocalFile::default()
                        .with_compression_method(CompressMethod::from(file_info.compression_method))
                        .with_compressed_size(file_info.compressed_size as u64)
                        .with_uncompressed_size(file_info.uncompressed_size as u64)
                        .with_stream(self.stream);
                    if let Ok(n) = self
                        .stream
                        .read(&mut file.file_name_buffer[..file_info.file_name_length as usize]) {
                        file.file_name_length = n;
                    }

                    // set next entry
                    self.next_entry_offset += file_info.len() as u64;

                    // seek to file data
                    let mut local_header_buf = [0u8; mem::size_of::<LocalFileHeader>()];
                    let _ = self.stream.seek(SeekFrom::Start(file_info.relative_offset_of_local_header as u64));
                    if matches!(self.stream.read(&mut local_header_buf), Ok(n) if n == local_header_buf.len()) {
                        if let Some(local_header) = unsafe { LocalFileHeader::from_bytes(&local_header_buf) } {
                            file.file_data_offset = file_info.relative_offset_of_local_header as u64 + local_header.len() as u64;
                            Some(file)
                        } else {
                            #[cfg(feature = "std")]
                            eprintln!("get LocalFileHeader from raw ptr({:02X?}) failed", local_header_buf);
                            None
                        }
                    } else {
                        #[cfg(feature = "std")]
                        eprintln!("read local header failed");
                        None
                    }
                } else {
                    #[cfg(feature = "std")]
                    eprintln!("get CentralFileHeader from raw ptr({:02X?}) failed", buf);
                    None
                }
            }
            Ok(_n) => {
                #[cfg(feature = "std")]
                eprintln!("no enough data: {}", _n);
                None
            }
            Err(_e) => {
                #[cfg(feature = "std")]
                eprintln!("stream read err: {}", _e);
                None
            }
        }
    }
}

pub struct SequentialParser<'a, S: Read, const N: usize = 128> {
    /// holding the file handle
    stream: &'a mut S,

    /// signature buffer
    buffer: [u8; LOCAL_FILE_HEADER_LEN],
    data_len_in_buffer: usize,
}

impl<'a, S: Read, const N: usize> SequentialParser<'a, S, N> {
    pub fn new(stream: &'a mut S) -> Self {
        Self {
            stream,
            buffer: [0; LOCAL_FILE_HEADER_LEN],
            data_len_in_buffer: 0,
        }
    }
}

impl<'a, S: Read, const N: usize> Iterator for SequentialParser<'a, S, N> {
    type Item = LocalFile<'a, S, N>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // read enough data
            let read_len = LOCAL_FILE_HEADER_LEN - self.data_len_in_buffer;
            match self.stream.read(&mut self.buffer[self.data_len_in_buffer..]) {
                Ok(n) => {
                    if n == 0 {
                        // FIXME: This will be an infinite loop if their is no more data
                        continue;
                    } else if n < read_len {
                        self.data_len_in_buffer += n;
                        continue;
                    } else {
                        self.data_len_in_buffer += n;
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "std")]
                    eprintln!("read local header failed({})", _e);
                    return None;
                }
            }

            // search LocalFileHeader
            // search signature
            let mut header_found = false;
            for (i, _v) in self.buffer.iter().take(self.data_len_in_buffer - 3).enumerate() {
                if self.buffer[i..i+4] == [0x50, 0x4b, 0x03, 0x04] {
                    unsafe {
                        let len = self.data_len_in_buffer - i;
                        ptr::copy(self.buffer.as_ptr().add(i),
                                  self.buffer.as_mut_ptr(),
                                  len);
                        self.data_len_in_buffer = len;
                        header_found = true;
                    }
                    // #[cfg(feature = "std")]
                    // println!("found signature at {}", i);
                    break;
                }
            }

            // save the last 3 bytes if header was not found
            if header_found == false {
                for i in 0..3 {
                    self.buffer[i] = self.buffer[self.data_len_in_buffer - 3 + i];
                }
                self.data_len_in_buffer = 3;
                continue;
            }

            // begin to parse if data is ready
            if self.data_len_in_buffer >= LOCAL_FILE_HEADER_LEN {
                break;
            }
        }

        // parse header
        if let Some(file_info) = unsafe { LocalFileHeader::from_bytes(&self.buffer) } {
            // #[cfg(feature = "std")]
            // dbg!(file_info);
            let mut file = LocalFile::default()
                .with_compression_method(CompressMethod::from(file_info.compression_method))
                .with_compressed_size(file_info.compressed_size as u64)
                .with_uncompressed_size(file_info.uncompressed_size as u64)
                .with_stream(self.stream);

            // read file name
            match self.stream.read_exact(&mut file.file_name_buffer[..file_info.file_name_length as usize]) {
                Ok(_) => file.file_name_length = file_info.file_name_length as usize,
                Err(_e) => {
                    #[cfg(feature = "std")]
                    eprintln!("read filename failed: {}", _e);
                },
            }

            // drop extra field
            {
                let mut len = file_info.extra_field_length as usize;
                let mut buf = [0u8; 16];
                loop {
                    let read_len = cmp::min(buf.len(), len);
                    if let Ok(n) = self.stream.read(&mut buf[..read_len]) {
                        len -= n;
                        if len == 0 {
                            break;
                        }
                    } else {
                        #[cfg(feature = "std")]
                        eprintln!("drop data read failed");
                        return None;
                    }
                }
            }

            // reset for next header
            self.data_len_in_buffer = 0;

            Some(file)
        } else {
            #[cfg(feature = "std")]
            eprintln!("get LocalFileHeader from raw ptr({:02X?}) failed", self.buffer);
            None
        }
    }
}


#[derive(Debug, Clone, Copy)]
pub enum ParserState {
    RecvHeader,
    RecvLocalFileHeader,
    RecvCentralFileHeader,
    RecvCentralDirEnd,
    RecvLocalFileName,
    RecvLocalFileExtraField,
    RecvLocalFileData,
}

pub enum DataStream<'a> {
    Start,
    Data(&'a [u8]),
    End,
}

pub struct PassiveParser<'a, S: Read, const N: usize> {
    /// header buffer
    buffer: [u8; CENTRAL_FILE_HEADER_LEN],
    buffer_data_index: usize,

    #[cfg(feature = "std")]
    zip_file_comment: Vec<u8>,

    pub localfile: Option<LocalFile<'a, S, N>>,

    localfile_index: i32,
    centralfile_index: i32,

    file_name_len: usize,
    file_name_index: usize,

    extra_field_len: usize,
    extra_field_index: usize,

    file_data_len: usize,
    file_data_index: usize,

    central_file_header_index: usize,
    central_file_header_len: usize,

    central_dir_end_index: usize,
    central_dir_end_len: usize,

    pub state: ParserState,
}

impl<'a, S: Read, const N: usize> PassiveParser<'a, S, N> {
    fn free_space(&self) -> usize {
        if self.buffer_data_index > self.buffer.len() {
            0
        } else {
            self.buffer.len() - self.buffer_data_index
        }
    }

    fn buffer_data_len(&self) -> usize {
        if self.buffer_data_index > self.buffer.len() {
            self.buffer.len()
        } else {
            self.buffer_data_index
        }
    }

    fn buffer_data(&self) -> &[u8] {
        &self.buffer[..self.buffer_data_len()]
    }

    fn append_bytes(&mut self, data: &[u8]) -> usize {
        let len = if data.len() > self.free_space() {
            self.free_space()
        } else {
            data.len()
        };
        (&mut self.buffer[self.buffer_data_index..self.buffer_data_index + len]).copy_from_slice(&data[..len]);
        self.buffer_data_index += len;
        len
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.state = ParserState::RecvHeader;

        #[cfg(feature = "std")]
        { self.zip_file_comment = Vec::new(); }

        self.buffer.fill(0);
        self.buffer_data_index = 0;

        self.localfile = None;
        self.localfile_index = 0;
        self.centralfile_index = 0;

        self.file_name_index = 0;
        self.file_name_len = 0;

        self.extra_field_index = 0;
        self.extra_field_len = 0;

        self.file_data_index = 0;
        self.file_data_len = 0;

        self.central_file_header_index = 0;
        self.central_file_header_len = 0;

        self.central_dir_end_index = 0;
        self.central_dir_end_len = 0;
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    pub fn localfile_index(&self) -> i32 {
        self.localfile_index
    }

    pub fn file_comment(&self) -> Result<&str, Utf8Error> {
        #[cfg(feature = "std")]
        { str::from_utf8(self.zip_file_comment.as_slice()) }
        #[cfg(not(feature = "std"))]
        { Ok("") }
    }

    pub fn feed_data<F>(&mut self, data: &[u8], mut on_event: F)
    where
        F: for<'b, 'c> FnMut(ParserEvent<'a, 'b, 'c, S, N>) -> bool,
    {
        let mut count = 0;
        let res: Result<usize, usize> = loop {
            // All data have being processed
            if count >= data.len() {
                break Ok(data.len());
            }
            match self.state {
                ParserState::RecvHeader => {
                    // queue data
                    let len = cmp::min(
                        4 - self.buffer_data_len(),
                        data.len() - count,
                    );
                    count += self.append_bytes(&data[count..count + len]);

                    // check signature
                    if self.buffer_data_len() < 4 {
                        continue;
                    }

                    // parse signature type
                    match Signature::try_from(self.buffer_data()) {
                        Err(err) => {
                            let res = on_event(ParserEvent::ParsingError(self.localfile_index, err));
                            if res == false {
                                break Err(count);
                            }
                        }
                        Ok(sig) => {
                            match sig {
                                Signature::LocalFileHeader => self.state = ParserState::RecvLocalFileHeader,
                                Signature::CentralFileHeader => self.state = ParserState::RecvCentralFileHeader,
                                Signature::CentralDirEnd => self.state = ParserState::RecvCentralDirEnd,
                            }
                        }
                    }
                }
                ParserState::RecvLocalFileHeader => {
                    // queue data
                    let len = cmp::min(
                        LOCAL_FILE_HEADER_LEN - self.buffer_data_len(),
                        data.len() - count,
                    );
                    count += self.append_bytes(&data[count..count + len]);

                    // header is not ready
                    if self.buffer_data_len() < LOCAL_FILE_HEADER_LEN {
                        continue;
                    }

                    // search LocalFileHeader
                    // {
                    //     search signature
                    //     let mut header_found = false;
                    //     for (i, _v) in self.buffer.iter().take(self.buffer_data_index - 3).enumerate() {
                    //         if self.buffer[i..i+4] == [0x50, 0x4b, 0x03, 0x04] {
                    //             unsafe {
                    //                 let len = self.buffer_data_index - i;
                    //                 ptr::copy(self.buffer.as_ptr().add(i),
                    //                         self.buffer.as_mut_ptr(),
                    //                         len);
                    //                 self.buffer_data_index = len;
                    //                 header_found = true;
                    //             }
                    //             // #[cfg(feature = "std")]
                    //             // println!("found signature at {}", i);
                    //             break;
                    //         }
                    //     }

                    //     save the last 3 bytes if header was not found
                    //     if header_found == false {
                    //         unsafe {
                    //             ptr::copy(
                    //                 self.buffer.as_ptr().add(self.buffer_data_len() - 3),
                    //                 self.buffer.as_mut_ptr(),
                    //                 3,
                    //             );
                    //         }
                    //         self.buffer_data_index = 3;
                    //         continue;
                    //     }

                    //     begin to parse if data is ready
                    //     if self.buffer_data_len() < LOCAL_FILE_HEADER_LEN {
                    //         continue;
                    //     }
                    // }

                    // parse header
                    if let Some(file_info) = unsafe {
                        LocalFileHeader::from_bytes(&self.buffer)
                    } {
                        // #[cfg(feature = "std")]
                        // dbg!(file_info);

                        // header is ready
                        self.state = ParserState::RecvLocalFileName;

                        let file = LocalFile::default()
                            .with_compression_method(CompressMethod::from(file_info.compression_method))
                            .with_compressed_size(file_info.compressed_size as u64)
                            .with_uncompressed_size(file_info.uncompressed_size as u64);
                        self.localfile.replace(file);
                        self.file_name_index = 0;
                        self.file_name_len = file_info.file_name_length as usize;
                        self.extra_field_index = 0;
                        self.extra_field_len = file_info.extra_field_length as usize;
                        self.file_data_index = 0;
                        self.file_data_len = file_info.compressed_size as usize;

                        // The data size in buffer must equal to LOCAL_FILE_HEADER_LEN
                        self.buffer_data_index = 0;
                    } else {
                        // #[cfg(feature = "std")]
                        // eprintln!("get LocalFileHeader from raw ptr({:02X?}) failed", self.buffer);

                        let err = ParsingError::InvalidLocalFileHeader;
                        let res = on_event(ParserEvent::ParsingError(self.localfile_index, err));

                        // drop all data
                        self.buffer_data_index = 0;

                        if res == false {
                            break Err(count);
                        }
                    }
                }
                ParserState::RecvLocalFileName => {
                    // if header is ready
                    if self.localfile.is_none() {
                        let err = ParsingError::LocalFileHeaderNotRecved(self.localfile_index);
                        let res = on_event(ParserEvent::ParsingError(self.localfile_index, err));
                        if res == false {
                            break Err(count);
                        }
                    }

                    // check file name len
                    if self.file_name_len > N {
                        let err = ParsingError::LocalFileNameTooLong(self.localfile_index, self.file_name_len as usize);
                        let res = on_event(ParserEvent::ParsingError(self.localfile_index, err));
                        if res == false {
                            break Err(count);
                        }
                    }

                    // save filename
                    if self.file_name_index >= self.file_name_len {
                        self.localfile.as_mut().unwrap().file_name_length = self.file_name_len;

                        self.state = ParserState::RecvLocalFileExtraField;
                    } else {
                        let len = cmp::min(
                            self.file_name_len - self.file_name_index,
                            data.len() - count,
                        );
                        self.localfile.as_mut().unwrap()
                            .file_name_buffer[self.file_name_index..self.file_name_index + len]
                            .as_mut()
                            .copy_from_slice(&data[count..count + len]);
                        self.file_name_index += len;

                        // count processed data
                        count += len;
                    }
                }
                ParserState::RecvLocalFileExtraField => {
                    if self.extra_field_index >= self.extra_field_len {
                        let res = on_event(ParserEvent::LocalFileHeader(self.localfile_index, self.localfile.as_ref().unwrap()));

                        self.state = ParserState::RecvLocalFileData;

                        if res == false {
                            break Err(count);
                        }
                    } else {
                        // fake save
                        let len = cmp::min(
                            self.extra_field_len - self.extra_field_index,
                            data.len() - count,
                        );
                        self.extra_field_index += len;

                        // count processed data
                        count += len;
                    }
                }
                ParserState::RecvLocalFileData => {
                    if self.file_data_index >= self.file_data_len {
                        let res = on_event(ParserEvent::LocalFileEnd(self.localfile_index));

                        self.localfile_index += 1;
                        self.state = ParserState::RecvHeader;

                        if res == false {
                            break Err(count);
                        }
                    } else {
                        // process
                        let len = cmp::min(
                            self.file_data_len - self.file_data_index,
                            data.len() - count,
                        );
                        let res = on_event(
                            ParserEvent::LocalFileData{
                                file_index: self.localfile_index,
                                offset: self.file_data_index,
                                data: &data[count..count + len],
                            }
                        );
                        self.file_data_index += len;

                        // count processed data
                        count += len;

                        if res == false {
                            break Err(count);
                        }
                    }
                }
                ParserState::RecvCentralFileHeader => {
                    if self.central_file_header_len == 0 {
                        let len = cmp::min(
                            CENTRAL_FILE_HEADER_LEN - self.buffer_data_len(),
                            data.len() - count,
                        );
                        count += self.append_bytes(&data[count..count + len]);
                        if self.buffer_data_len() < CENTRAL_FILE_HEADER_LEN {
                            continue;
                        }

                        // parse
                        if let Some(header) = unsafe { CentralFileHeader::from_bytes(&self.buffer) } {
                            self.central_file_header_len = header.len();
                            self.central_file_header_index = self.buffer_data_len();

                            // drop all data
                            self.buffer_data_index = 0;
                        } else {
                            let err = ParsingError::InvalidCentralFileHeader;
                            let res = on_event(ParserEvent::ParsingError(self.localfile_index, err));

                            // drop all data
                            self.buffer_data_index = 0;

                            if res == false {
                                break Err(count);
                            }
                        }
                    } else {
                        if self.central_file_header_index >= self.central_file_header_len {
                            self.centralfile_index += 1;
                            self.central_file_header_index = 0;
                            self.central_file_header_len = 0;
                            self.state = ParserState::RecvHeader;
                        } else {
                            let len = cmp::min(
                                self.central_file_header_len - self.central_file_header_index,
                                data.len() - count,
                            );
                            count += len;
                            self.central_file_header_index += len;
                        }
                    }
                }
                ParserState::RecvCentralDirEnd => {
                    if self.central_dir_end_len == 0 {
                        let len = cmp::min(
                            CENTRAL_DIR_END_LEN - self.buffer_data_len(),
                            data.len() - count,
                        );
                        count += self.append_bytes(&data[count..count + len]);
                        if self.buffer_data_len() < CENTRAL_DIR_END_LEN {
                            continue;
                        }

                        // parse
                        if let Some(header) = unsafe { CentralDirEnd::from_bytes(&self.buffer) } {
                            self.central_dir_end_len = header.len();
                            self.central_dir_end_index = self.buffer_data_len();

                            // drop all data
                            self.buffer_data_index = 0;
                        } else {
                            let err = ParsingError::InvalidCentralDirEnd;
                            let res = on_event(ParserEvent::ParsingError(self.localfile_index, err));

                            // drop all data
                            self.buffer_data_index = 0;

                            if res == false {
                                break Err(count);
                            }
                        }
                    } else {
                        if self.central_dir_end_index >= self.central_dir_end_len {
                            self.central_dir_end_index = 0;
                            self.central_dir_end_len = 0;
                            self.state = ParserState::RecvHeader;
                        } else {
                            let len = cmp::min(
                                self.central_dir_end_len - self.central_dir_end_index,
                                data.len() - count,
                            );

                            #[cfg(feature = "std")]
                            self.zip_file_comment.extend(&data[count..count + len]);

                            count += len;
                            self.central_dir_end_index += len;
                        }
                    }
                }
            };
        };

        // report consumed len
        if let Err(n) = res {
            on_event(ParserEvent::UserCancel(-1, n));
        }
    }
}

impl<'a, S: Read, const N: usize> Default for PassiveParser<'a, S, N> {
    fn default() -> Self {
        Self {
            state: ParserState::RecvHeader,

            #[cfg(feature = "std")]
            zip_file_comment: Vec::new(),

            buffer: [0; CENTRAL_FILE_HEADER_LEN],
            buffer_data_index: 0,

            localfile: None,
            localfile_index: 0,
            centralfile_index: 0,

            file_name_index: 0,
            file_name_len: 0,

            extra_field_index: 0,
            extra_field_len: 0,

            file_data_index: 0,
            file_data_len: 0,

            central_file_header_index: 0,
            central_file_header_len: 0,

            central_dir_end_index: 0,
            central_dir_end_len: 0,
        }
    }
}

/// Prelude of zip_parser
pub mod prelude {
    pub use crate::{
        LocalFile, LocalFileOps,
        Parser, ParsingError, ParserState, ParserEvent, DataStream,
        SequentialParser, SeekingParser, PassiveParser,
    };
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::prelude::*;

    use crate::{CentralDirEnd, CentralFileHeader, LocalFileHeader, Signature};

    #[test]
    fn parse_file_list() {
    }
}
