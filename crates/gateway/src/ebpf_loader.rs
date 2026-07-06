// Minimal eBPF loader. Parses an ELF relocatable file (ET_REL) and extracts
// BPF program instructions from the .text section plus the symbol table and
// license string. This is NOT a verifier or JIT — it just pulls the bytes a
// runtime needs to feed into the kernel BPF() syscall.
//
// Refs:
//   - Linux kernel BPF type_format.h (BTF encoding notes)
//   - libelf "minimal ELF section walker"

#[derive(Debug, PartialEq, Eq)]
pub struct ElfHeader {
    pub ei_class: u8,
    pub ei_data: u8,
    pub ei_version: u8,
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
    pub is_64: bool,
    pub is_le: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Section {
    pub name_offset: u32,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_link: u32,
    pub sh_info: u32,
    pub sh_addralign: u64,
    pub sh_entsize: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name_offset: u32,
    pub info: u8,
    pub other: u8,
    pub shndx: u16,
    pub value: u64,
    pub size: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EbpfProg {
    pub name: String,
    pub instructions: Vec<u64>,
    pub license: Option<String>,
    pub symbol_value: u64,
}

const ELFCLASS32: u8 = 1;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ELFDATA2MSB: u8 = 2;
const ET_REL: u16 = 1;
const EM_BPF: u16 = 247;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_PROGBITS: u32 = 1;
const SHF_ALLOC: u64 = 0x2;

pub fn parse_elf(input: &[u8]) -> Result<(ElfHeader, Vec<Section>), String> {
    if input.len() < 52 { return Err("input too short".into()); }
    if &input[0..4] != b"\x7fELF" { return Err("not an ELF file".into()); }
    let ei_class = input[4];
    let ei_data = input[5];
    let ei_version = input[6];
    let is_64 = ei_class == ELFCLASS64;
    let is_le = ei_data == ELFDATA2LSB;
    let read_u16 = |off: usize| -> u16 { if is_le { u16::from_le_bytes([input[off], input[off+1]]) } else { u16::from_be_bytes([input[off], input[off+1]]) } };
    let read_u32 = |off: usize| -> u32 { if is_le { u32::from_le_bytes([input[off], input[off+1], input[off+2], input[off+3]]) } else { u32::from_be_bytes([input[off], input[off+1], input[off+2], input[off+3]]) } };
    let read_u64 = |off: usize| -> u64 { if is_le { u64::from_le_bytes(input[off..off+8].try_into().unwrap()) } else { u64::from_be_bytes(input[off..off+8].try_into().unwrap()) } };
    let (e_type_off, e_machine_off, e_version_off, e_entry_off, e_phoff_off, e_shoff_off, e_flags_off, e_ehsize_off, e_phentsize_off, e_phnum_off, e_shentsize_off, e_shnum_off, e_shstrndx_off) = if is_64 {
        (16, 18, 20, 24, 32, 40, 48, 52, 54, 56, 58, 60, 62)
    } else {
        (16, 18, 20, 24, 28, 32, 36, 40, 42, 44, 46, 48, 50)
    };
    if input.len() < e_shstrndx_off + 2 { return Err("header truncated".into()); }
    let e_type = read_u16(e_type_off);
    let e_machine = read_u16(e_machine_off);
    let e_version = read_u32(e_version_off);
    let e_entry = if is_64 { read_u64(e_entry_off) } else { read_u32(e_entry_off) as u64 };
    let e_phoff = if is_64 { read_u64(e_phoff_off) } else { read_u32(e_phoff_off) as u64 };
    let e_shoff = if is_64 { read_u64(e_shoff_off) } else { read_u32(e_shoff_off) as u64 };
    let e_flags = read_u32(e_flags_off);
    let e_ehsize = read_u16(e_ehsize_off);
    let e_phentsize = read_u16(e_phentsize_off);
    let e_phnum = read_u16(e_phnum_off);
    let e_shentsize = read_u16(e_shentsize_off);
    let e_shnum = read_u16(e_shnum_off);
    let e_shstrndx = read_u16(e_shstrndx_off);
    let header = ElfHeader {
        ei_class, ei_data, ei_version, e_type, e_machine, e_version,
        e_entry, e_phoff, e_shoff, e_flags, e_ehsize,
        e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx,
        is_64, is_le,
    };
    let shentsize = if is_64 { 64 } else { 40 };
    if e_shoff == 0 || e_shnum == 0 {
        return Ok((header, Vec::new()));
    }
    let mut sections = Vec::with_capacity(e_shnum as usize);
    for i in 0..e_shnum as usize {
        let off = e_shoff as usize + i * shentsize;
        if off + shentsize > input.len() { return Err(format!("section header {} past EOF", i)); }
        let s = Section {
            name_offset: read_u32(off),
            sh_type: read_u32(off + 4),
            sh_flags: if is_64 { read_u64(off + 8) } else { read_u32(off + 8) as u64 },
            sh_addr: if is_64 { read_u64(off + 16) } else { read_u32(off + 12) as u64 },
            sh_offset: if is_64 { read_u64(off + 24) } else { read_u32(off + 16) as u64 },
            sh_size: if is_64 { read_u64(off + 32) } else { read_u32(off + 20) as u64 },
            sh_link: read_u32(if is_64 { off + 40 } else { off + 24 }),
            sh_info: read_u32(if is_64 { off + 44 } else { off + 28 }),
            sh_addralign: if is_64 { read_u64(off + 48) } else { read_u32(off + 32) as u64 },
            sh_entsize: if is_64 { read_u64(off + 56) } else { read_u32(off + 36) as u64 },
            data: Vec::new(),
        };
        sections.push(s);
    }
    for s in sections.iter_mut() {
        if s.sh_type == SHT_NOBITS { continue; }
        let start = s.sh_offset as usize;
        let end = start + s.sh_size as usize;
        if end <= input.len() && end > start {
            s.data = input[start..end].to_vec();
        }
    }
    Ok((header, sections))
}

const SHT_NOBITS: u32 = 8;

pub fn parse_symbols(symtab: &Section) -> Result<Vec<Symbol>, String> {
    if symtab.sh_type != SHT_SYMTAB { return Err("not a symbol table".into()); }
    if symtab.data.is_empty() { return Ok(Vec::new()); }
    let entsize = if symtab.sh_entsize > 0 { symtab.sh_entsize as usize } else { 24 };
    let mut syms = Vec::new();
    let mut i = 0;
    while i + entsize <= symtab.data.len() {
        let chunk = &symtab.data[i..i+entsize];
        let (name_offset, info, other, shndx, value, size) = if entsize == 24 {
            let name = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            (name, chunk[4], chunk[5], u16::from_le_bytes([chunk[6], chunk[7]]),
             u64::from_le_bytes(chunk[8..16].try_into().unwrap()),
             u64::from_le_bytes(chunk[16..24].try_into().unwrap()))
        } else {
            let name = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            (name, chunk[4], chunk[5], u16::from_le_bytes([chunk[6], chunk[7]]),
             u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]) as u64,
             u32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]) as u64)
        };
        syms.push(Symbol { name_offset, info, other, shndx, value, size });
        i += entsize;
    }
    Ok(syms)
}

pub fn read_strtab(strtab: &Section, offset: u32) -> String {
    let start = offset as usize;
    if start >= strtab.data.len() { return String::new(); }
    let mut end = start;
    while end < strtab.data.len() && strtab.data[end] != 0 { end += 1; }
    String::from_utf8_lossy(&strtab.data[start..end]).to_string()
}

pub fn find_section<'a>(sections: &'a [Section], shstrtab: &Section, name: &str) -> Option<&'a Section> {
    sections.iter().find(|s| read_strtab(shstrtab, s.name_offset) == name)
}

pub fn extract_programs(header: &ElfHeader, sections: &[Section], shstrtab: &Section, strtab: &Section) -> Vec<EbpfProg> {
    if header.e_machine != EM_BPF { return Vec::new(); }
    let mut out = Vec::new();
    for sec in sections {
        if sec.sh_type != SHT_PROGBITS { continue; }
        if sec.sh_flags & SHF_ALLOC == 0 { continue; }
        let name = read_strtab(shstrtab, sec.name_offset);
        if !name.starts_with('.') { continue; }
        let mut instructions = Vec::with_capacity(sec.data.len() / 8);
        for chunk in sec.data.chunks(8) {
            if chunk.len() < 8 { break; }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(chunk);
            instructions.push(u64::from_le_bytes(bytes));
        }
        let license = if let Some(lic_sec) = sections.iter().find(|s| read_strtab(shstrtab, s.name_offset) == "license") {
            if let Ok(s) = std::str::from_utf8(&lic_sec.data) { Some(s.trim_end_matches('\0').to_string()) } else { None }
        } else { None };
        out.push(EbpfProg { name, instructions, license, symbol_value: 0 });
    }
    let symtab = sections.iter().find(|s| s.sh_type == SHT_SYMTAB);
    if let Some(symtab) = symtab {
        if let Ok(syms) = parse_symbols(symtab) {
            for sym in &syms {
                let sym_name = read_strtab(strtab, sym.name_offset);
                if sym.info >> 4 == 2 && sym.size > 0 {
                    for prog in out.iter_mut() {
                        if prog.name == format!(".text") || sym_name == prog.name.trim_start_matches('.') {
                            prog.symbol_value = sym.value;
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn not_elf() {
        assert!(parse_elf(b"not an elf").is_err());
    }
    #[test] fn parse_elf64_header() {
        let mut elf = Vec::new();
        elf.extend_from_slice(b"\x7fELF");
        elf.push(2);
        elf.push(1);
        elf.push(1);
        elf.extend_from_slice(&[0u8; 9]);
        elf.extend_from_slice(&1u16.to_le_bytes()); // ET_REL
        elf.extend_from_slice(&247u16.to_le_bytes()); // EM_BPF
        elf.extend_from_slice(&1u32.to_le_bytes());
        elf.extend_from_slice(&0u64.to_le_bytes());
        elf.extend_from_slice(&0u64.to_le_bytes());
        elf.extend_from_slice(&0u64.to_le_bytes());
        elf.extend_from_slice(&0u32.to_le_bytes());
        elf.extend_from_slice(&64u16.to_le_bytes());
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_phentsize
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_phnum
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx
        let (h, secs) = parse_elf(&elf).unwrap();
        assert!(h.is_64);
        assert!(h.is_le);
        assert_eq!(h.e_machine, 247);
        assert_eq!(secs.len(), 0);
    }
    #[test] fn header_truncated() {
        assert!(parse_elf(&[0u8; 10]).is_err());
    }
    #[test] fn find_section_empty() {
        let shstrtab = Section { name_offset: 0, sh_type: SHT_STRTAB, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 0, sh_link: 0, sh_info: 0, sh_addralign: 0, sh_entsize: 0, data: b"\x00.text\x00".to_vec() };
        let text = Section { name_offset: 1, sh_type: SHT_PROGBITS, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 0, sh_link: 0, sh_info: 0, sh_addralign: 0, sh_entsize: 0, data: vec![] };
        assert_eq!(find_section(&[text], &shstrtab, ".text").unwrap().sh_type, SHT_PROGBITS);
        assert!(find_section(&[], &shstrtab, "nope").is_none());
    }
    #[test] fn read_strtab_works() {
        let sec = Section { name_offset: 0, sh_type: SHT_STRTAB, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 0, sh_link: 0, sh_info: 0, sh_addralign: 0, sh_entsize: 0, data: b"\x00hello\x00".to_vec() };
        assert_eq!(read_strtab(&sec, 1), "hello");
        assert_eq!(read_strtab(&sec, 0), "");
        assert_eq!(read_strtab(&sec, 100), "");
    }
    #[test] fn parse_empty_symtab() {
        let s = Section { name_offset: 0, sh_type: SHT_SYMTAB, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 0, sh_link: 0, sh_info: 0, sh_addralign: 0, sh_entsize: 0, data: vec![] };
        assert!(parse_symbols(&s).unwrap().is_empty());
    }
    #[test] fn reject_non_symtab() {
        let s = Section { name_offset: 0, sh_type: SHT_PROGBITS, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 0, sh_link: 0, sh_info: 0, sh_addralign: 0, sh_entsize: 0, data: vec![] };
        assert!(parse_symbols(&s).is_err());
    }
    #[test] fn parse_symtab_64() {
        // Build a symtab with one 24-byte entry: name_off=1, info=0, other=0, shndx=1, value=0x100, size=0x80
        let mut data = vec![0u8; 24];
        data[0..4].copy_from_slice(&1u32.to_le_bytes());
        data[4] = 0x12; // STT_FUNC + STB_GLOBAL
        data[5] = 0;
        data[6..8].copy_from_slice(&1u16.to_le_bytes());
        data[8..16].copy_from_slice(&0x100u64.to_le_bytes());
        data[16..24].copy_from_slice(&0x80u64.to_le_bytes());
        let s = Section { name_offset: 0, sh_type: SHT_SYMTAB, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 24, sh_link: 0, sh_info: 0, sh_addralign: 8, sh_entsize: 24, data };
        let syms = parse_symbols(&s).unwrap();
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].value, 0x100);
        assert_eq!(syms[0].size, 0x80);
    }
    #[test] fn extract_no_bpf_returns_empty() {
        // Build ELF64 header for EM_X86_64 (machine=62) so e_machine != EM_BPF
        let mut elf = Vec::new();
        elf.extend_from_slice(b"\x7fELF");
        elf.push(2);
        elf.push(1);
        elf.push(1);
        elf.extend_from_slice(&[0u8; 9]);
        elf.extend_from_slice(&1u16.to_le_bytes()); // ET_REL
        elf.extend_from_slice(&62u16.to_le_bytes()); // EM_X86_64
        elf.extend_from_slice(&1u32.to_le_bytes());
        elf.extend_from_slice(&0u64.to_le_bytes());
        elf.extend_from_slice(&0u64.to_le_bytes());
        elf.extend_from_slice(&0u64.to_le_bytes());
        elf.extend_from_slice(&0u32.to_le_bytes());
        elf.extend_from_slice(&64u16.to_le_bytes());
        elf.extend_from_slice(&0u16.to_le_bytes());
        elf.extend_from_slice(&0u16.to_le_bytes());
        elf.extend_from_slice(&0u16.to_le_bytes());
        elf.extend_from_slice(&0u16.to_le_bytes());
        elf.extend_from_slice(&0u16.to_le_bytes());
        let (h, secs) = parse_elf(&elf).unwrap();
        let shstrtab = Section { name_offset: 0, sh_type: SHT_STRTAB, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 0, sh_link: 0, sh_info: 0, sh_addralign: 0, sh_entsize: 0, data: vec![] };
        let strtab = Section { name_offset: 0, sh_type: SHT_STRTAB, sh_flags: 0, sh_addr: 0, sh_offset: 0, sh_size: 0, sh_link: 0, sh_info: 0, sh_addralign: 0, sh_entsize: 0, data: vec![] };
        let progs = extract_programs(&h, &secs, &shstrtab, &strtab);
        assert!(progs.is_empty());
    }
}