use std::fmt;
use strings;
use std::string::String;
use std::io::prelude::*;
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use crc::{Hasher32, crc32};

// HEADER

const HEADER_SIZE: usize = 448;
const HEADER_ID_32: &str = "Inno Setup Uninstall Log (b)";
const HEADER_ID_64: &str = "Inno Setup Uninstall Log (b) 64-bit";
const HIGHEST_SUPPORTED_VERSION: i32 = 1048;

pub struct Header {
	id: String,       // 64 bytes
	app_id: String,   // 128
	app_name: String, // 128
	version: i32,
	pub num_recs: usize,
	end_offset: u32,
	flags: u32,
	crc: u32,
}

impl fmt::Debug for Header {
	fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		write!(
			formatter,
			"Header
id: {}
app id: {}
app name: {}
version: {}
num recs: {}
end offset: {}
flags: 0x{:x}
crc: 0x{:x}",
			self.id,
			self.app_id,
			self.app_name,
			self.version,
			self.num_recs,
			self.end_offset,
			self.flags,
			self.crc,
		)
	}
}

impl Header {
	pub fn from_reader(reader: &mut Read) -> Header {
		let mut buf = [0; HEADER_SIZE];
		reader.read_exact(&mut buf).expect("read error");
		let mut read: &[u8] = &buf;

		let id = strings::read_utf8_string(&mut read, 64).expect("header id");
		let app_id = strings::read_utf8_string(&mut read, 128).expect("header app id");
		let app_name = strings::read_utf8_string(&mut read, 128).expect("header app name");
		let version = read.read_i32::<LittleEndian>().expect("header version");
		let num_recs = read.read_i32::<LittleEndian>().expect("header num recs") as usize;
		let end_offset = read.read_u32::<LittleEndian>().expect("header end offset");
		let flags = read.read_u32::<LittleEndian>().expect("header flags");

		let mut reserved = [0; 108];
		read.read_exact(&mut reserved).expect("header reserved");
		let crc = read.read_u32::<LittleEndian>().expect("header crc");

		let mut digest = crc32::Digest::new(crc32::IEEE);
		digest.write(&buf[..HEADER_SIZE - 4]);
		let actual_crc = digest.sum32();

		if actual_crc != crc {
			panic!("header crc32 check failed");
		}

		match id.as_ref() {
			HEADER_ID_32 => (),
			HEADER_ID_64 => (),
			_ => panic!("header id not valid"),
		}

		if version > HIGHEST_SUPPORTED_VERSION {
			panic!("header version not supported");
		}

		Header {
			id,
			app_id,
			app_name,
			version,
			num_recs,
			end_offset,
			flags,
			crc,
		}
	}
}

// FILE REC

#[derive(Copy, Clone)]
pub enum UninstallRecTyp {
	UserDefined = 0x01,
	StartInstall = 0x10,
	EndInstall = 0x11,
	CompiledCode = 0x20,
	Run = 0x80,
	DeleteDirOrFiles = 0x81,
	DeleteFile = 0x82,
	DeleteGroupOrItem = 0x83,
	IniDeleteEntry = 0x84,
	IniDeleteSection = 0x85,
	RegDeleteEntireKey = 0x86,
	RegClearValue = 0x87,
	RegDeleteKeyIfEmpty = 0x88,
	RegDeleteValue = 0x89,
	DecrementSharedCount = 0x8A,
	RefreshFileAssoc = 0x8B,
	MutexCheck = 0x8C,
}

impl UninstallRecTyp {
	fn from(i: u16) -> UninstallRecTyp {
		match i {
			0x01 => UninstallRecTyp::UserDefined,
			0x10 => UninstallRecTyp::StartInstall,
			0x11 => UninstallRecTyp::EndInstall,
			0x20 => UninstallRecTyp::CompiledCode,
			0x80 => UninstallRecTyp::Run,
			0x81 => UninstallRecTyp::DeleteDirOrFiles,
			0x82 => UninstallRecTyp::DeleteFile,
			0x83 => UninstallRecTyp::DeleteGroupOrItem,
			0x84 => UninstallRecTyp::IniDeleteEntry,
			0x85 => UninstallRecTyp::IniDeleteSection,
			0x86 => UninstallRecTyp::RegDeleteEntireKey,
			0x87 => UninstallRecTyp::RegClearValue,
			0x88 => UninstallRecTyp::RegDeleteKeyIfEmpty,
			0x89 => UninstallRecTyp::RegDeleteValue,
			0x8A => UninstallRecTyp::DecrementSharedCount,
			0x8B => UninstallRecTyp::RefreshFileAssoc,
			0x8C => UninstallRecTyp::MutexCheck,
			_ => panic!(""),
		}
	}
}

pub struct FileRec {
	pub typ: UninstallRecTyp,
	extra_data: u32,
	data: Vec<u8>,
}

impl<'a> fmt::Debug for FileRec {
	fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		write!(
			formatter,
			"FileRec 0x{:x} 0x{:x} {} bytes",
			self.typ as u32,
			self.extra_data as u32,
			self.data.len(),
		)
	}
}

impl<'a> FileRec {
	pub fn from_reader(reader: &mut Read) -> FileRec {
		let typ = reader.read_u16::<LittleEndian>().expect("file rec typ");
		let extra_data = reader
			.read_u32::<LittleEndian>()
			.expect("file rec extra data");
		let data_size = reader
			.read_u32::<LittleEndian>()
			.expect("file rec data size") as usize;

		if data_size > 0x8000000 {
			panic!("file rec data size too large {}", data_size);
		}

		let mut data = vec![0; data_size];
		reader.read_exact(&mut data).expect("file rec data");

		let typ = UninstallRecTyp::from(typ);

		FileRec {
			typ,
			extra_data,
			data,
		}
	}

	fn get_string(&self) -> (String, usize) {
		let mut read_slice: &[u8] = &self.data;
		let reader: &mut Read = &mut read_slice;

		let first = reader.read_u8().expect("file rec data first byte");
		assert!(first == 0xfe);

		let size = reader
			.read_i32::<LittleEndian>()
			.expect("file rec data size");
		assert!(size < 0);

		let slice: &[u8] = &self.data;
		let last = slice[slice.len() - 1];
		assert!(last == 0xff);

		let old_size = -size as usize;
		assert!(old_size % 2 == 0);

		let mut u16data: Vec<u16> = vec![0; old_size / 2];
		// println!("{}, {}, {}", size, 5 + size, self.data.len());

		LittleEndian::read_u16_into(&slice[5..5 + old_size], &mut u16data);

		(
			String::from_utf16(&u16data).expect("file rec data string"),
			old_size,
		)
	}

	pub fn rebase(&mut self, from: &str, to: &str) {
		let (mut path, old_size) = self.get_string();

		if path.starts_with(from) {
			path = [to, &path[from.len()..]].join("");
		}

		let u16data: Vec<u16> = path.encode_utf16().collect();
		let new_size = u16data.len() * 2;
		let mut data: Vec<u8> = vec![0; self.data.len() - old_size + new_size];

		{
			let mut slice: &mut [u8] = &mut data[..];
			let writer: &mut Write = &mut slice;

			writer.write_u8(0xfe).expect("file rec data first byte");
			writer
				.write_i32::<LittleEndian>(-(new_size as i32))
				.expect("file rec data size");
		}

		{
			let slice = &mut data[5..5 + new_size];
			LittleEndian::write_u16_into(&u16data, slice);
		}

		{
			let old_rest = &self.data[5 + old_size..];
			let new_rest = &mut data[5 + new_size..];
			new_rest.copy_from_slice(old_rest);
		}

		self.data = data;
	}
}
