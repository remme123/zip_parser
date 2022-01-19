#![cfg_attr(all(not(test), not(feature = "std")), no_std)]

use core::fmt::{Display, Formatter};
use core::mem;
use core::ptr;
use core::slice;
use core::str;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "std")]
use std::{io, fs};

type ReadResult<'a> = Result<usize, &'a str>;
pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> ReadResult;
}

#[cfg(feature = "std")]
impl<T> Read for T
where
    T: io::Read,
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
    fn seek(&mut self, _pos: SeekFrom) -> Result<u64, &str> {
        Err("unimplemented")
    }

    fn rewind(&mut self) -> Result<(), &str> {
        if self.seek(SeekFrom::Start(0)).is_ok() {
            Ok(())
        } else {
            Err("seek to beginning failed")
        }
    }

    fn stream_len(&mut self) -> Option<u64> {
        None
    }
}

#[cfg(feature = "std")]
impl<T> Seek for T
where
    T: io::Seek,
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

#[derive(Debug)]
struct ZipFileStream<'a, S: Read + Seek> {
    stream: *mut S,
    seek_available: bool,
    stream_len: u64,

     _marker: PhantomData<&'a S>
}

impl<'a, S: Read + Seek> Deref for ZipFileStream<'a, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        unsafe {
            self.stream.as_ref().unwrap()
        }
    }
}

impl<'a, S: Read + Seek> DerefMut for ZipFileStream<'a, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            self.stream.as_mut().unwrap()
        }
    }
}

impl<'a, S: Read + Seek> Read for ZipFileStream<'a, S> {
    fn read(&mut self, buf: &mut [u8]) -> ReadResult {
        self.deref_mut().read(buf)
    }
}

impl<'a, S: Read + Seek> Seek for ZipFileStream<'a, S> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, &str> {
        self.deref_mut().seek(pos)
    }
}

#[derive(Debug)]
pub struct LocalFile<'a, S: Read + Seek> {
    file_name: [u8; 128],
    file_name_length: usize,
    file_data_offset: u64,

    pub compression_method: u16,
    pub compressed_size: u64,
    pub uncompressed_size: u64,

    stream: ZipFileStream<'a, S>,
}

impl<'a, S: Read + Seek> LocalFile<'a, S> {
    pub unsafe fn file_name(&self) -> &str {
        str::from_utf8_unchecked(&self.file_name[..self.file_name_length])
    }

    pub fn read(&mut self, buf: &mut [u8]) -> ReadResult {
        self.stream.read(buf)
    }
}

// #[cfg(feature = "std")]
// impl<'a, S: Read + Seek> io::Read for LocalFile<'a, S> {
//     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//         todo!()
//     }
// }

// impl<S:  Read + Seek> Default for LocalFile<S> {
//     fn default() -> Self {
//         LocalFile {
//             file_name: [0; 128],
//             file_name_length: 0,
//
//             compression_method: 0,
//             compressed_size: 0,
//             uncompressed_size: 0,
//         }
//     }
// }

pub struct Parser<S: Read + Seek> {
    /// It will be None when no central directory was found
    pub number_of_files: Option<usize>,

    central_directory_offset: u64,
    /// offset relative to the central dir
    next_entry_offset: u64,

    /// holding the file handle
    stream: S,
    seek_available: bool,
    stream_len: u64,
}

impl<S: Read + Seek> Parser<S> {
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
        }
    }
}

pub struct Iter<'a, S: Read + Seek> {
    stream: ZipFileStream<'a, S>,
}

impl<'a, S: Read + Seek> Iterator for &'a mut Parser<S> {
    type Item = LocalFile<'a, S>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.seek_available {
            let _ = self.stream.seek(SeekFrom::Start(self.central_directory_offset + self.next_entry_offset));
            let mut buf = [0u8; mem::size_of::<CentralFileHeader>()];
            match self.stream.read(&mut buf) {
                Ok(n) if n == buf.len() => {
                    if let Some(file_info) = unsafe { CentralFileHeader::from_raw_ptr(&buf) } {
                        #[cfg(feature = "std")]
                        dbg!(file_info);
                        let mut file = LocalFile {
                            file_name: [0; 128],
                            file_name_length: 0,
                            file_data_offset: file_info.relative_offset_of_local_header as u64 + file_info.local_header_len() as u64,
                            compression_method: file_info.compression_method,
                            compressed_size: file_info.compressed_size as u64,
                            uncompressed_size: file_info.uncompressed_size as u64,
                            stream: ZipFileStream {
                                stream: &mut self.stream,
                                stream_len: self.stream_len,
                                seek_available: self.seek_available,
                                _marker: PhantomData,
                            }
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
                                let data_offset = file_info.relative_offset_of_local_header as u64 + local_header.len() as u64;
                                if file.file_data_offset != data_offset {
                                    #[cfg(feature = "std")] {
                                        eprintln!("file header len does not match(central <-> local): {} != {}",
                                                  file.file_data_offset, data_offset);
                                        eprintln!("in central header: {:02X?}", &file_info);
                                        eprintln!("in lcoal header: {:02X?}", &local_header);
                                    }
                                    file.file_data_offset = data_offset;
                                }
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
            todo!();
            None
        }
    }
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
