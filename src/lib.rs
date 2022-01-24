//! # zip_parser
//! Zip file format parser implemented by rust. support stream parsing, no_std environment.
//!
//! The `Parser` will search central directory at the end of zip file if `Seek` is available.
//! Also, It supports sequence read parsing when `Seek` is not available.
//! The type which implements `std::io::Read` implements `Read` in `std` env, and so is the `Seek`.
//! ## example
//! ### stream_parsing
//! 1. from `stdin`
//!     ```bash
//!     cat test.zip | cargo run --features="std" --example stream_parsing
//!     ```
//!     or even you can cat multiple zip files:
//!     ```bash
//!     cat test.zip test.zip | cargo run --features="std" --example stream_parsing
//!     ```
//! 1. from file
//!     ```bash
//!     cargo run --features="std" --example stream_parsing -- test.zip
//!     ```

#![feature(specialization)]
#![cfg_attr(all(not(test), not(feature = "std")), no_std)]

use core::fmt::{Display, Formatter};
use core::mem;
use core::ptr;
use core::slice;
use core::str;
use core::marker::PhantomData;
use core::convert::{From, TryFrom};

#[cfg(feature = "std")]
use std::{io, fs};

pub type ReadResult = Result<usize, &'static str>;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> ReadResult;

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), &'static str> {
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
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<T> Read for T where T: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> ReadResult {
        self.read(buf).or(Err("std.io.read err"))
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
    fn seek(&mut self, _pos: SeekFrom) -> Result<u64, &str>;

    fn rewind(&mut self) -> Result<(), &str>;

    fn stream_len(&mut self) -> Option<u64>;
}

impl<T> Seek for T {
    default fn seek(&mut self, _pos: SeekFrom) -> Result<u64, &str> {
        Err("unimplemented")
    }

    default fn rewind(&mut self) -> Result<(), &str> {
        if self.seek(SeekFrom::Start(0)).is_ok() {
            Ok(())
        } else {
            Err("seek to the beginning failed")
        }
    }

    default fn stream_len(&mut self) -> Option<u64> {
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

#[repr(C)]
#[derive(Debug, Copy, Clone)]
enum Signature {
    Unknown = 0,
    LocalFileHeader = 0x04034b50,
    CentralFileHeader = 0x02014b50,
    CentralDirEnd = 0x06054b50,
}

impl Default for Signature {
    fn default() -> Self {
        Signature::Unknown
    }
}

impl From<u32> for Signature {
    fn from(value: u32) -> Self {
        match value {
            0x04034b50 => Signature::LocalFileHeader,
            0x02014b50 => Signature::CentralFileHeader,
            0x06054b50 => Signature::CentralDirEnd,
            _ => Signature::Unknown,
        }
    }
}

impl From<[u8; 4]> for Signature {
    fn from(value: [u8; 4]) -> Self {
        u32::from_le_bytes([value[0], value[1], value[2], value[3]]).into()
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < 4 {
            return Err("no enough data");
        }
        Ok([value[0], value[1], value[2], value[3]].into())
    }
}

#[allow(dead_code)]
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

    pub unsafe fn from_raw_ptr(ptr: &[u8]) -> Option<&Self> {
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

#[allow(dead_code)]
#[repr(packed)]
#[derive(Debug, Copy, Clone, Default)]
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

    pub unsafe fn from_raw_ptr(ptr: &[u8]) -> Option<&Self> {
        (ptr.as_ptr() as *const Self).as_ref().and_then(|h| {
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

    pub unsafe fn get_file_comment<'a>(&self) -> &'a str {
        let base = self as *const Self as *const u8;
        let b = slice::from_raw_parts(
            base.offset(
                (mem::size_of::<Self>()
                    + self.file_name_length as usize
                    + self.extra_field_length as usize) as isize,
            ),
            self.file_comment_length as usize,
        );
        str::from_utf8_unchecked(b)
    }
}

#[allow(dead_code)]
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

    pub unsafe fn from_raw_ptr(ptr: &[u8]) -> Option<&Self> {
        (ptr.as_ptr() as *const Self).as_ref().and_then(|h| {
            if matches!(h.signature, Signature::CentralDirEnd) {
                Some(h)
            } else {
                None
            }
        })
    }

    pub unsafe fn get_zip_file_comment<'a>(&self) -> &'a str {
        let base = self as *const Self as *const u8;
        let b = slice::from_raw_parts(
            base.offset(mem::size_of::<Self>() as isize),
            self.zip_file_comment_length as usize,
        );
        str::from_utf8_unchecked(b)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct LocalFile<'a, S: Read + Seek> {
    file_name: [u8; 128],
    file_name_length: usize,
    file_data_offset: u64,

    pub compression_method: u16,
    pub compressed_size: u64,
    pub uncompressed_size: u64,

    stream: *mut S,
    _marker: PhantomData<&'a mut S>,
}

impl<'a, S: Read + Seek> LocalFile<'a, S> {
    pub unsafe fn file_name(&self) -> &str {
        str::from_utf8_unchecked(&self.file_name[..self.file_name_length])
    }

    pub fn file_size(&self) -> u64 {
        self.compressed_size
    }

    pub fn read(&mut self, buf: &mut [u8]) -> ReadResult {
        unsafe {
            self.stream.as_mut().unwrap().read(buf)
        }
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), &'static str> {
        unsafe {
            self.stream.as_mut().unwrap().read_exact(buf)
        }
    }
}

// #[cfg(feature = "std")]
// impl<'a, S: Read + Seek> io::Read for LocalFile<'a, S> {
//     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//         todo!()
//     }
// }

#[allow(dead_code)]
pub struct Parser<'a, S: Read + Seek> {
    /// It will be None when no central directory was found
    pub number_of_files: Option<usize>,

    central_directory_offset: u64,
    /// offset relative to the central dir
    next_entry_offset: u64,

    /// holding the file handle
    stream: S,
    seek_available: bool,
    stream_len: u64,

    /// signature buffer
    buffer: [u8; mem::size_of::<LocalFileHeader>()],
    data_len_in_buffer: usize,

    _marker: PhantomData<&'a S>,
}

impl<'a, S: Read + Seek> Parser<'a, S> {
    pub fn new(mut stream: S) -> Self {
        // seek to the start of central directory
        let mut seek_available = false;
        let mut stream_len = 0u64;
        let mut central_directory_offset = 0u64;
        let mut number_of_files = None;
        if let Some(len) = stream.stream_len() {
            stream_len = len;
            const READ_LEN: usize = mem::size_of::<CentralDirEnd>();
            if let Ok(_) = stream.seek(SeekFrom::Start(stream_len - READ_LEN as u64)) {
                let mut buf = [0u8; READ_LEN];
                if matches!(stream.read(&mut buf), Ok(n) if n == buf.len()) {
                    if matches!([buf[0], buf[1], buf[2], buf[3]].into(), Signature::CentralDirEnd) {
                        let central_dir = unsafe { CentralDirEnd::from_raw_ptr(&buf).unwrap() };
                        let _ = stream.seek(SeekFrom::Start(central_dir.central_directory_offset as u64));
                        seek_available = true;
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
                eprintln!("seek is unavailable");
            }
        } else {
            #[cfg(feature = "std")]
            eprintln!("stream_len is unavailable");
        }

        Self {
            stream,
            seek_available,
            stream_len,
            central_directory_offset,
            next_entry_offset: 0,
            number_of_files,
            buffer: [0; mem::size_of::<LocalFileHeader>()],
            data_len_in_buffer: 0,
            _marker: PhantomData,
        }
    }
}

impl<'a, S: Read + Seek> Iterator for Parser<'a, S> {
    type Item = LocalFile<'a, S>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.seek_available {
            // seek read
            let _ = self.stream.seek(
                SeekFrom::Start(self.central_directory_offset + self.next_entry_offset)
            );
            let mut buf = [0u8; mem::size_of::<CentralFileHeader>()];
            match self.stream.read(&mut buf) {
                Ok(n) if n == buf.len() => {
                    if let Some(file_info) = unsafe { CentralFileHeader::from_raw_ptr(&buf) } {
                        #[cfg(feature = "std")]
                        dbg!(file_info);
                        let mut file = LocalFile {
                            file_name: [0; 128],
                            file_name_length: 0,
                            file_data_offset: 0,
                            compression_method: file_info.compression_method,
                            compressed_size: file_info.compressed_size as u64,
                            uncompressed_size: file_info.uncompressed_size as u64,
                            stream: &mut self.stream,
                            _marker: PhantomData,
                        };
                        if let Ok(n) = self
                            .stream
                            .read(&mut file.file_name[..file_info.file_name_length as usize]) {
                            file.file_name_length = n;
                        }

                        // set next entry
                        self.next_entry_offset += file_info.len() as u64;

                        // seek to file data
                        let mut local_header_buf = [0u8; mem::size_of::<LocalFileHeader>()];
                        let _ = self.stream.seek(SeekFrom::Start(file_info.relative_offset_of_local_header as u64));
                        if matches!(self.stream.read(&mut local_header_buf), Ok(n) if n == local_header_buf.len()) {
                            if let Some(local_header) = unsafe { LocalFileHeader::from_raw_ptr(&local_header_buf) } {
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
        } else {
            // sequential read
            loop {
                // read enough data
                let read_len = self.buffer.len() - self.data_len_in_buffer;
                if let Ok(n) = self.stream.read(&mut self.buffer[self.data_len_in_buffer..]) {
                    if n == 0 {
                        return None;
                    } else if n < read_len {
                        self.data_len_in_buffer += n;
                        continue;
                    } else {
                        self.data_len_in_buffer += n;
                    }
                } else {
                    #[cfg(feature = "std")]
                    eprintln!("read local header failed");
                    return None;
                }

                // search LocalFileHeader
                // search signature
                let mut header_found = false;
                for (i, _v) in self.buffer.iter().take(self.data_len_in_buffer - 3).enumerate() {
                    if self.buffer[i..i+4] == [0x50, 0x4b, 0x03, 0x04] {
                        unsafe {
                            let len = self.data_len_in_buffer - i;
                            ptr::copy(self.buffer.as_ptr().offset(i as isize),
                                      self.buffer.as_mut_ptr(),
                                      len);
                            self.data_len_in_buffer = len;
                            header_found = true;
                        }
                        #[cfg(feature = "std")]
                        println!("found signature at {}", i);
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
                if self.data_len_in_buffer == self.buffer.len() {
                    break;
                }
            }

            // parse header
            if let Some(file_info) = unsafe { LocalFileHeader::from_raw_ptr(&self.buffer) } {
                #[cfg(feature = "std")]
                dbg!(file_info);
                let mut file = LocalFile {
                    file_name: [0; 128],
                    file_name_length: 0,
                    file_data_offset: 0,
                    compression_method: file_info.compression_method,
                    compressed_size: file_info.compressed_size as u64,
                    uncompressed_size: file_info.uncompressed_size as u64,
                    stream: &mut self.stream,
                    _marker: PhantomData,
                };
                match self.stream.read_exact(&mut file.file_name[..file_info.file_name_length as usize]) {
                    Ok(_) => file.file_name_length = file_info.file_name_length as usize,
                    Err(_e) => {
                        #[cfg(feature = "std")]
                        eprintln!("read filename failed: {}", _e);
                    },
                }

                // drop data unprocessed
                {
                    let mut len = file_info.extra_field_length as usize;
                    let mut buf = [0u8; 16];
                    loop {
                        let read_len = if len > 16 { 16 } else { len };
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
}

/// Prelude of zip_parser
pub mod prelude {
    pub use crate::{Parser, LocalFile};
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
