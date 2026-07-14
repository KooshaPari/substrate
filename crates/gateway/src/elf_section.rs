//! Minimal ELF section header walker (32-bit and 64-bit).
//!
//! Parses the ELF magic + global header enough to identify class (32/64),
//! data encoding (LE/BE), and machine (e.g. EM_X86_64 = 0x3e). Walks the
//! section header table via `e_shoff` / `e_shentsize` / `e_shnum` and
//! resolves each entry's name against `e_shstrndx`.
//!
//! Reference: System V ABI gabi41.pdf §"ELF Header" and §"Section Header Table".
//!
//! Limits of this implementation:
//!
//! - Only reads class (32/64), data (LE/BE), version, and machine from the
//!   ELF header; program headers are NOT parsed.
//! - Section data is copied into memory via `data: Vec<u8>` (no memory mapping).
//! - Compressed sections, extended numbering (SHN_LORESERVE), and section
//!   groups are out of scope.

/// Parsed section table entry.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Section {
    /// Section name resolved against `.shstrtab`. Empty when the lookup
    /// section is missing.
    pub name: String,
    /// Section type (`sh_type`).
    pub sh_type: u32,
    /// Section attribute flags (`sh_flags`), already widened to `u64`.
    pub sh_flags: u64,
    /// Section virtual address in memory (`sh_addr`).
    pub sh_addr: u64,
    /// Byte offset of section contents in the file (`sh_offset`).
    pub sh_offset: u64,
    /// Section size in bytes (`sh_size`).
    pub sh_size: u64,
    /// Section header table index link (`sh_link`).
    pub sh_link: u32,
    /// Extra info field (`sh_info`).
    pub sh_info: u32,
    /// Alignment (`sh_addralign`).
    pub sh_addralign: u64,
    /// Entry size for tables (`sh_entsize`).
    pub sh_entsize: u64,
    /// Section payload (up to `sh_size` bytes starting at `sh_offset`).
    pub data: Vec<u8>,
}

/// Top-level ELF header metadata that callers usually care about.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ElfMeta {
    /// `true` for ELF64 (class = 2).
    pub is_64: bool,
    /// `true` for little-endian (data = 1).
    pub is_le: bool,
    /// Target architecture (`e_machine`).
    pub e_machine: u16,
    /// Number of section header entries (`e_shnum`).
    pub e_shnum: u16,
    /// Index of the section-name string table (`e_shstrndx`).
    pub e_shstrndx: u16,
}

// ELF magic.
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

// Section types (selected subset — full list is in gabi41 §"Section Types").
pub const SHT_NULL: u32 = 0;
pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_STRTAB: u32 = 3;
pub const SHT_RELA: u32 = 4;
pub const SHT_HASH: u32 = 5;
pub const SHT_DYNAMIC: u32 = 6;
pub const SHT_NOTE: u32 = 7;
pub const SHT_NOBITS: u32 = 8;
pub const SHT_REL: u32 = 9;
pub const SHT_SHLIB: u32 = 10;
pub const SHT_DYNSYM: u32 = 11;
pub const SHT_INIT_ARRAY: u32 = 14;
pub const SHT_FINI_ARRAY: u32 = 15;
pub const SHT_PREINIT_ARRAY: u32 = 16;
pub const SHT_GROUP: u32 = 17;
pub const SHT_SYMTAB_SHNDX: u32 = 18;

/// Return a human-readable name for well-known `sh_type` values, or
/// `"SHT_<n>"` for unknown ones.
pub fn section_type_str(t: u32) -> &'static str {
    match t {
        SHT_NULL => "SHT_NULL",
        SHT_PROGBITS => "SHT_PROGBITS",
        SHT_SYMTAB => "SHT_SYMTAB",
        SHT_STRTAB => "SHT_STRTAB",
        SHT_RELA => "SHT_RELA",
        SHT_HASH => "SHT_HASH",
        SHT_DYNAMIC => "SHT_DYNAMIC",
        SHT_NOTE => "SHT_NOTE",
        SHT_NOBITS => "SHT_NOBITS",
        SHT_REL => "SHT_REL",
        SHT_SHLIB => "SHT_SHLIB",
        SHT_DYNSYM => "SHT_DYNSYM",
        SHT_INIT_ARRAY => "SHT_INIT_ARRAY",
        SHT_FINI_ARRAY => "SHT_FINI_ARRAY",
        SHT_PREINIT_ARRAY => "SHT_PREINIT_ARRAY",
        SHT_GROUP => "SHT_GROUP",
        SHT_SYMTAB_SHNDX => "SHT_SYMTAB_SHNDX",
        _ => "SHT_UNKNOWN",
    }
}

/// Parse just the global ELF header and return [`ElfMeta`].
///
/// Returns an error string if `input` is not a valid ELF file (bad magic,
/// unknown class/data encoding, missing section header fields).
pub fn parse_meta(input: &[u8]) -> Result<ElfMeta, String> {
    // 16 bytes for the magic + class + data + version + osabi + abi_ver + pad
    // + 8 bytes for the rest of the leading fields. We need at least up to
    // e_machine (offset 18) plus the next 2 bytes of pad before we can read
    // e_shoff / e_shentsize / e_shnum / e_shstrndx. We parse enough to answer
    // ElfMeta.
    if input.len() < 4 + 8 + 6 {
        return Err("ELF header truncated (< 18 bytes)".into());
    }
    if input[0..4] != ELF_MAGIC {
        return Err(format!("not an ELF file (magic = {:02x?})", &input[0..4]));
    }
    let class = input[4];
    let data = input[5];
    let is_64 = match class {
        1 => false,
        2 => true,
        other => return Err(format!("unknown ELF class {other} (not 1/2)")),
    };
    let is_le = match data {
        1 => true,
        2 => false,
        other => return Err(format!("unknown data encoding {other} (not 1/2)")),
    };
    // Version is at byte 6 (must be 1); we accept any value to keep the parser
    // forgiving for malformed files.
    // Header layout for ELF32: e_type(2) e_machine(2) e_version(4) e_entry(4)
    //   e_phoff(4) e_shoff(4) e_flags(4) e_ehsize(2) e_phentsize(2) e_phnum(2)
    //   e_shentsize(2) e_shnum(2) e_shstrndx(2) = 52 bytes total.
    // For ELF64: e_type(2) e_machine(2) e_version(4) e_entry(8) e_phoff(8)
    //   e_shoff(8) e_flags(4) e_ehsize(2) e_phentsize(2) e_phnum(2)
    //   e_shentsize(2) e_shnum(2) e_shstrndx(2) = 64 bytes total.
    let header_len = if is_64 { 64 } else { 52 };
    if input.len() < header_len {
        return Err(format!("ELF header truncated (< {header_len} bytes)"));
    }
    let read_u16 = |off: usize| -> u16 {
        if is_le {
            u16::from_le_bytes([input[off], input[off + 1]])
        } else {
            u16::from_be_bytes([input[off], input[off + 1]])
        }
    };
    let e_machine = read_u16(18);
    // e_shentsize at offset 46 for ELF32, 58 for ELF64.
    // e_shnum at offset 48 for ELF32, 60 for ELF64.
    // e_shstrndx at offset 50 for ELF32, 62 for ELF64.
    let (off_shentsize, off_shnum, off_shstrndx) = if is_64 { (58, 60, 62) } else { (46, 48, 50) };
    let _shentsize = read_u16(off_shentsize);
    let e_shnum = read_u16(off_shnum);
    let e_shstrndx = read_u16(off_shstrndx);
    Ok(ElfMeta {
        is_64,
        is_le,
        e_machine,
        e_shnum,
        e_shstrndx,
    })
}

/// Parse the section header table.
///
/// Returns one [`Section`] per entry in the table, with name resolved against
/// `e_shstrndx`. If `e_shstrndx` is invalid or the named string table is
/// absent, every `Section.name` will be empty.
///
/// Returns an error string if the section header table is truncated or its
/// offsets land outside the input.
pub fn parse_sections(input: &[u8]) -> Result<Vec<Section>, String> {
    let meta = parse_meta(input)?;
    if meta.e_shnum == 0 {
        return Ok(Vec::new());
    }

    // Section header entry size differs between 32/64.
    let (shentsize, header_len) = if meta.is_64 {
        (64usize, 64usize)
    } else {
        (40usize, 52usize)
    };
    if input.len() < header_len {
        return Err("ELF header truncated".into());
    }
    // e_shoff is the file offset of the section header table.
    let read_u32 = |off: usize| -> u32 {
        if meta.is_le {
            u32::from_le_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        } else {
            u32::from_be_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        }
    };
    let read_u64 = |off: usize| -> u64 {
        if meta.is_le {
            u64::from_le_bytes([
                input[off],
                input[off + 1],
                input[off + 2],
                input[off + 3],
                input[off + 4],
                input[off + 5],
                input[off + 6],
                input[off + 7],
            ])
        } else {
            u64::from_be_bytes([
                input[off],
                input[off + 1],
                input[off + 2],
                input[off + 3],
                input[off + 4],
                input[off + 5],
                input[off + 6],
                input[off + 7],
            ])
        }
    };

    // e_shoff offset: ELF32 = 32, ELF64 = 40.
    let off_shoff = if meta.is_64 { 40 } else { 32 };
    let e_shoff = if meta.is_64 {
        read_u64(off_shoff)
    } else {
        read_u32(off_shoff) as u64
    };

    let total = e_shoff
        .checked_add(
            (meta.e_shnum as u64)
                .checked_mul(shentsize as u64)
                .ok_or("e_shnum overflow")?,
        )
        .ok_or("e_shoff + table size overflow")?;
    if total as usize > input.len() {
        return Err(format!(
            "section header table truncated: need {} bytes, have {}",
            total,
            input.len()
        ));
    }

    // Parse the string-table section (e_shstrndx) first so we can resolve names.
    let strtab = if meta.e_shstrndx == 0 || meta.e_shstrndx >= meta.e_shnum {
        Vec::new()
    } else {
        let s = read_section_entry(input, e_shoff, shentsize, meta.e_shstrndx, &meta)?;
        if s.sh_type != SHT_STRTAB || s.sh_type == SHT_NOBITS {
            Vec::new()
        } else {
            s.data.clone()
        }
    };

    let mut out = Vec::with_capacity(meta.e_shnum as usize);
    for i in 0..meta.e_shnum {
        let raw = read_section_entry(input, e_shoff, shentsize, i, &meta)?;
        let name = if strtab.is_empty() {
            String::new()
        } else {
            lookup_str(&strtab, raw.name_offset)
        };
        out.push(Section {
            name,
            sh_type: raw.sh_type,
            sh_flags: raw.sh_flags,
            sh_addr: raw.sh_addr,
            sh_offset: raw.sh_offset,
            sh_size: raw.sh_size,
            sh_link: raw.sh_link,
            sh_info: raw.sh_info,
            sh_addralign: raw.sh_addralign,
            sh_entsize: raw.sh_entsize,
            data: raw.data,
        });
    }
    Ok(out)
}

/// Internal: raw fields we collect from the section header entry before
/// resolving the name. Pulled out so the string-table fetch can use the same
/// path without duplicating offsets.
struct RawSection {
    name_offset: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
    data: Vec<u8>,
}

fn read_section_entry(
    input: &[u8],
    e_shoff: u64,
    shentsize: usize,
    index: u16,
    meta: &ElfMeta,
) -> Result<RawSection, String> {
    let entry_off = e_shoff as usize + index as usize * shentsize;
    let end = entry_off + shentsize;
    if end > input.len() {
        return Err(format!(
            "section entry {index} truncated (offset {entry_off})"
        ));
    }
    let read_u32 = |off: usize| -> u32 {
        if meta.is_le {
            u32::from_le_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        } else {
            u32::from_be_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        }
    };
    let read_u64 = |off: usize| -> u64 {
        if meta.is_le {
            u64::from_le_bytes([
                input[off],
                input[off + 1],
                input[off + 2],
                input[off + 3],
                input[off + 4],
                input[off + 5],
                input[off + 6],
                input[off + 7],
            ])
        } else {
            u64::from_be_bytes([
                input[off],
                input[off + 1],
                input[off + 2],
                input[off + 3],
                input[off + 4],
                input[off + 5],
                input[off + 6],
                input[off + 7],
            ])
        }
    };

    // ELF32 section entry layout (40 bytes):
    //   sh_name(4) sh_type(4) sh_flags(4) sh_addr(4) sh_offset(4)
    //   sh_size(4) sh_link(4) sh_info(4) sh_addralign(4) sh_entsize(4)
    // ELF64 section entry layout (64 bytes):
    //   sh_name(4) sh_type(4) sh_flags(8) sh_addr(8) sh_offset(8)
    //   sh_size(8) sh_link(4) sh_info(4) sh_addralign(8) sh_entsize(8)
    let name_offset = read_u32(entry_off);
    let sh_type = read_u32(entry_off + 4);
    let (sh_flags, sh_addr, sh_offset, sh_size, sh_link, sh_info, sh_addralign, sh_entsize) =
        if meta.is_64 {
            (
                read_u64(entry_off + 8),
                read_u64(entry_off + 16),
                read_u64(entry_off + 24),
                read_u64(entry_off + 32),
                read_u32(entry_off + 40),
                read_u32(entry_off + 44),
                read_u64(entry_off + 48),
                read_u64(entry_off + 56),
            )
        } else {
            (
                read_u32(entry_off + 8) as u64,
                read_u32(entry_off + 12) as u64,
                read_u32(entry_off + 16) as u64,
                read_u32(entry_off + 20) as u64,
                read_u32(entry_off + 24),
                read_u32(entry_off + 28),
                read_u32(entry_off + 32) as u64,
                read_u32(entry_off + 36) as u64,
            )
        };
    let data = if sh_type == SHT_NOBITS || sh_size == 0 {
        Vec::new()
    } else {
        let off = sh_offset as usize;
        let size = sh_size as usize;
        if off
            .checked_add(size)
            .map(|e| e <= input.len())
            .unwrap_or(false)
        {
            input[off..off + size].to_vec()
        } else {
            return Err(format!(
                "section data truncated: offset {off} size {size} > input"
            ));
        }
    };
    Ok(RawSection {
        name_offset,
        sh_type,
        sh_flags,
        sh_addr,
        sh_offset,
        sh_size,
        sh_link,
        sh_info,
        sh_addralign,
        sh_entsize,
        data,
    })
}

/// Resolve a NUL-terminated C string at `offset` inside `strtab`. Returns the
/// empty string when the offset lies outside the table or the string is not
/// terminated.
fn lookup_str(strtab: &[u8], offset: u32) -> String {
    let start = offset as usize;
    if start >= strtab.len() {
        return String::new();
    }
    let rest = &strtab[start..];
    match rest.iter().position(|&b| b == 0) {
        Some(end) => String::from_utf8_lossy(&rest[..end]).into_owned(),
        None => String::from_utf8_lossy(rest).into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a minimal ELF32 little-endian header.
    fn mk_elf32_le(e_machine: u16, e_shnum: u16, e_shstrndx: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&ELF_MAGIC);
        v.push(1); // class = ELF32
        v.push(1); // data = LE
        v.push(1); // version
        v.push(0); // osabi
        v.extend_from_slice(&[0u8; 8]); // abi_version + pad
        v.extend_from_slice(&2u16.to_le_bytes()); // e_type = ET_EXEC
        v.extend_from_slice(&e_machine.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes()); // e_version
        v.extend_from_slice(&0u32.to_le_bytes()); // e_entry
        v.extend_from_slice(&0u32.to_le_bytes()); // e_phoff
        v.extend_from_slice(&52u32.to_le_bytes()); // e_shoff
        v.extend_from_slice(&0u32.to_le_bytes()); // e_flags
        v.extend_from_slice(&52u16.to_le_bytes()); // e_ehsize
        v.extend_from_slice(&0u16.to_le_bytes()); // e_phentsize
        v.extend_from_slice(&0u16.to_le_bytes()); // e_phnum
        v.extend_from_slice(&40u16.to_le_bytes()); // e_shentsize
        v.extend_from_slice(&e_shnum.to_le_bytes());
        v.extend_from_slice(&e_shstrndx.to_le_bytes());
        v
    }

    // Build a minimal ELF64 little-endian header.
    fn mk_elf64_le(e_machine: u16, e_shnum: u16, e_shstrndx: u16, e_shoff: u64) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&ELF_MAGIC);
        v.push(2); // class = ELF64
        v.push(1); // data = LE
        v.push(1); // version
        v.push(0); // osabi
        v.extend_from_slice(&[0u8; 8]); // abi_version + pad
        v.extend_from_slice(&2u16.to_le_bytes()); // e_type = ET_EXEC
        v.extend_from_slice(&e_machine.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes()); // e_version
        v.extend_from_slice(&0u64.to_le_bytes()); // e_entry
        v.extend_from_slice(&0u64.to_le_bytes()); // e_phoff
        v.extend_from_slice(&e_shoff.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // e_flags
        v.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
        v.extend_from_slice(&0u16.to_le_bytes()); // e_phentsize
        v.extend_from_slice(&0u16.to_le_bytes()); // e_phnum
        v.extend_from_slice(&64u16.to_le_bytes()); // e_shentsize
        v.extend_from_slice(&e_shnum.to_le_bytes());
        v.extend_from_slice(&e_shstrndx.to_le_bytes());
        v
    }

    // Build one ELF32 section entry. sh_type = 3 (STRTAB) by default.
    fn mk_sec32(name_off: u32, sh_type: u32, offset: u32, size: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&name_off.to_le_bytes());
        v.extend_from_slice(&sh_type.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // sh_flags
        v.extend_from_slice(&0u32.to_le_bytes()); // sh_addr
        v.extend_from_slice(&offset.to_le_bytes());
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // sh_link
        v.extend_from_slice(&0u32.to_le_bytes()); // sh_info
        v.extend_from_slice(&1u32.to_le_bytes()); // sh_addralign
        v.extend_from_slice(&0u32.to_le_bytes()); // sh_entsize
        v
    }

    // Build one ELF64 section entry.
    fn mk_sec64(name_off: u32, sh_type: u32, offset: u64, size: u64) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&name_off.to_le_bytes());
        v.extend_from_slice(&sh_type.to_le_bytes());
        v.extend_from_slice(&0u64.to_le_bytes()); // sh_flags
        v.extend_from_slice(&0u64.to_le_bytes()); // sh_addr
        v.extend_from_slice(&offset.to_le_bytes());
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // sh_link
        v.extend_from_slice(&0u32.to_le_bytes()); // sh_info
        v.extend_from_slice(&1u64.to_le_bytes()); // sh_addralign
        v.extend_from_slice(&0u64.to_le_bytes()); // sh_entsize
        v
    }

    #[test]
    fn parse_elf64_le_header() {
        let buf = mk_elf64_le(0x3e, 0, 0, 64);
        let m = parse_meta(&buf).unwrap();
        assert!(m.is_64);
        assert!(m.is_le);
        assert_eq!(m.e_machine, 0x3e); // EM_X86_64
        assert_eq!(m.e_shnum, 0);
        assert_eq!(m.e_shstrndx, 0);
    }

    #[test]
    fn parse_elf32_le_header() {
        let buf = mk_elf32_le(0x03, 0, 0);
        let m = parse_meta(&buf).unwrap();
        assert!(!m.is_64);
        assert!(m.is_le);
        assert_eq!(m.e_machine, 0x03); // EM_386
        assert_eq!(m.e_shnum, 0);
        assert_eq!(m.e_shstrndx, 0);
    }

    #[test]
    fn parse_sections_with_shstrtab() {
        // Build ELF32 with two sections: NULL entry + a STRTAB at index 1.
        // The strtab contains "\0.text\0.shstrtab\0" — offsets:
        //   0 -> "", 1 -> ".text", 7 -> ".shstrtab".
        let mut buf = mk_elf32_le(0x03, 2, 1);
        let strtab_off = buf.len() as u32;
        let strtab = b"\0.text\0.shstrtab\0";
        buf.extend_from_slice(strtab);

        let strtab_sec_off = buf.len() as u32;
        let mut sec0 = mk_sec32(0, SHT_NULL, 0, 0);
        let mut sec1 = mk_sec32(7, SHT_STRTAB, strtab_off, strtab.len() as u32);
        buf.append(&mut sec0);
        buf.append(&mut sec1);
        // e_shoff points to the start of sec0 (= strtab_sec_off).
        buf[32..36].copy_from_slice(&strtab_sec_off.to_le_bytes());

        let sections = parse_sections(&buf).unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "");
        assert_eq!(sections[1].name, ".shstrtab");
        assert_eq!(sections[1].sh_type, SHT_STRTAB);
        assert_eq!(sections[1].data, strtab);
    }

    #[test]
    fn parse_sections_empty_table() {
        let buf = mk_elf32_le(0x03, 0, 0);
        let sections = parse_sections(&buf).unwrap();
        assert!(sections.is_empty());
    }

    #[test]
    fn reject_non_elf_magic() {
        let buf = b"NOTELF1234567890";
        assert!(parse_meta(buf).is_err());
    }

    #[test]
    fn reject_truncated_header() {
        let buf = b"\x7fELF\x01\x01\x01\x00";
        assert!(parse_meta(buf).is_err());
    }

    #[test]
    fn big_endian_magic() {
        // Build ELF32 with data=2 (big-endian).
        let mut v = Vec::new();
        v.extend_from_slice(&ELF_MAGIC);
        v.push(1); // ELF32
        v.push(2); // BE
        v.push(1);
        v.push(0);
        v.extend_from_slice(&[0u8; 8]);
        v.extend_from_slice(&2u16.to_be_bytes());
        v.extend_from_slice(&0x14u16.to_be_bytes()); // EM_PPC
        v.extend_from_slice(&1u32.to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes());
        v.extend_from_slice(&52u32.to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes());
        v.extend_from_slice(&52u16.to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend_from_slice(&40u16.to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes());
        let m = parse_meta(&v).unwrap();
        assert!(!m.is_le);
        assert_eq!(m.e_machine, 0x14);
    }

    #[test]
    fn section_type_names() {
        assert_eq!(section_type_str(SHT_SYMTAB), "SHT_SYMTAB");
        assert_eq!(section_type_str(SHT_STRTAB), "SHT_STRTAB");
        assert_eq!(section_type_str(SHT_PROGBITS), "SHT_PROGBITS");
        assert_eq!(section_type_str(SHT_NOBITS), "SHT_NOBITS");
        assert_eq!(section_type_str(0xabcdef), "SHT_UNKNOWN");
    }

    #[test]
    fn parse_sections_elf64_with_data() {
        // Build ELF64 with 3 sections. Layout:
        //   header(64) | sec_table(3*64) | strtab(17) | payload(2)
        // The section table sits BEFORE the strtab data — exactly the layout
        // a real toolchain emits.
        let mut buf = mk_elf64_le(0x3e, 3, 1, 64); // e_shoff patched below
        let sec_table_off = buf.len() as u64; // 64
                                              // Patch e_shoff = sec_table_off (still 64 in this case).
        buf[40..48].copy_from_slice(&sec_table_off.to_le_bytes());

        // Three section entries — sh_size / sh_offset get back-patched.
        let mut sec0 = mk_sec64(0, SHT_NULL, 0, 0);
        let mut sec1 = mk_sec64(0, SHT_STRTAB, 0, 0); // patched later
        let mut sec2 = mk_sec64(0, SHT_PROGBITS, 0, 0); // patched later
        buf.append(&mut sec0);
        buf.append(&mut sec1);
        buf.append(&mut sec2);

        let strtab_off = buf.len() as u64;
        let strtab = b"\0.text\0.shstrtab\0";
        buf.extend_from_slice(strtab);

        let payload_off = buf.len() as u64;
        buf.extend_from_slice(b"hi");

        // Patch sec1: name_offset=7 ("\0.text\0.shstrtab" -> index 7 is ".shstrtab"),
        // sh_offset=strtab_off, sh_size=strtab.len().
        let sec1_off = (sec_table_off + 64) as usize;
        buf[sec1_off..sec1_off + 4].copy_from_slice(&7u32.to_le_bytes());
        buf[sec1_off + 24..sec1_off + 32].copy_from_slice(&strtab_off.to_le_bytes());
        buf[sec1_off + 32..sec1_off + 40].copy_from_slice(&(strtab.len() as u64).to_le_bytes());

        // Patch sec2: name_offset=1 (".text"), sh_offset=payload_off, sh_size=2.
        let sec2_off = (sec_table_off + 128) as usize;
        buf[sec2_off..sec2_off + 4].copy_from_slice(&1u32.to_le_bytes());
        buf[sec2_off + 24..sec2_off + 32].copy_from_slice(&payload_off.to_le_bytes());
        buf[sec2_off + 32..sec2_off + 40].copy_from_slice(&2u64.to_le_bytes());

        let sections = parse_sections(&buf).unwrap();
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].name, "");
        assert_eq!(sections[1].name, ".shstrtab");
        assert_eq!(sections[1].sh_type, SHT_STRTAB);
        assert_eq!(sections[2].name, ".text");
        assert_eq!(sections[2].sh_type, SHT_PROGBITS);
        assert_eq!(sections[2].data, b"hi".to_vec());
    }
}
