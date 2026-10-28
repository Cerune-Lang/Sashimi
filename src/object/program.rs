//! Typed program layout, object generation and section-relative provenance.
//! The caller owns instruction selection, ABI and the meaning of OriginId.
use super::{Fixup, Object, encode, format};
use crate::{
    Target,
    typed::{Instruction, Operand},
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SymbolId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OriginId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Text,
    ReadOnlyData,
    Data,
}
impl Section {
    fn index(self) -> usize {
        match self {
            Self::Text => 0,
            Self::ReadOnlyData => 1,
            Self::Data => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linkage {
    Local,
    Global,
    External,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub linkage: Linkage,
}

/// Empty origins require synthetic = true. Synthetic entries may also carry origins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub origins: Vec<OriginId>,
    pub synthetic: bool,
}
impl Provenance {
    pub fn source(origin: OriginId) -> Self {
        Self {
            origins: vec![origin],
            synthetic: false,
        }
    }
    pub fn synthetic() -> Self {
        Self {
            origins: vec![],
            synthetic: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Section(Section),
    Label(SymbolId),
    Instruction(Instruction),
    Data(Vec<u8>),
    /// UTF-8 bytes followed by one NUL; embedded NUL bytes are retained.
    CString(String),
    /// Exact little-endian bits, rendered as .long; no implicit alignment.
    Data32(u32),
    /// Exact little-endian bits, rendered as .quad; no implicit alignment.
    Data64(u64),
    /// Power-of-two exponent; maximum 16 (ELF), 13 (COFF).
    Align(u8),
    Marker,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub provenance: Provenance,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub target: Target,
    pub symbols: Vec<Symbol>,
    pub entries: Vec<Entry>,
}
impl Program {
    pub fn new(target: Target) -> Self {
        Self {
            target,
            symbols: vec![],
            entries: vec![],
        }
    }
    pub fn declare(&mut self, name: impl Into<String>, linkage: Linkage) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol {
            name: name.into(),
            linkage,
        });
        id
    }
    pub fn push(&mut self, kind: EntryKind, provenance: Provenance) -> EntryId {
        let id = EntryId(self.entries.len());
        self.entries.push(Entry { kind, provenance });
        id
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub section: Section,
    pub offset: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteRange {
    pub section: Section,
    pub start: u64,
    pub end: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryLocation {
    pub entry: EntryId,
    pub provenance: Provenance,
    /// Section selections have no range; labels and markers have an empty range.
    pub range: Option<ByteRange>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolLocation {
    pub symbol: SymbolId,
    pub position: Option<Position>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    ResolvedRelative,
    ElfPc32,
    ElfPlt32,
    CoffRel32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub entry: EntryId,
    pub symbol: SymbolId,
    pub field: ByteRange,
    /// PC-relative addend measured from the displacement field, including trailing immediates.
    pub addend: i64,
    pub kind: ReferenceKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub object: Vec<u8>,
    pub entries: Vec<EntryLocation>,
    pub symbols: Vec<SymbolLocation>,
    pub references: Vec<Reference>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Validate,
    Encode,
    Resolve,
    Object,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramDiagnostic {
    pub phase: Phase,
    pub message: String,
    pub entry: Option<EntryId>,
    pub origins: Vec<OriginId>,
    pub symbol: Option<SymbolId>,
}
impl std::fmt::Display for ProgramDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.phase)?;
        if let Some(id) = self.entry {
            write!(f, " entry {}", id.0)?;
        }
        if let Some(id) = self.symbol {
            write!(f, " symbol {}", id.0)?;
        }
        write!(f, ": {}", self.message)
    }
}
impl std::error::Error for ProgramDiagnostic {}
impl Program {
    fn error(
        &self,
        phase: Phase,
        entry: Option<EntryId>,
        symbol: Option<SymbolId>,
        message: impl Into<String>,
    ) -> ProgramDiagnostic {
        ProgramDiagnostic {
            phase,
            message: message.into(),
            entry,
            symbol,
            origins: entry
                .map_or_else(Vec::new, |id| self.entries[id.0].provenance.origins.clone()),
        }
    }
}
struct Layout {
    object: Object,
    entries: Vec<EntryLocation>,
    symbols: Vec<SymbolLocation>,
    references: Vec<Reference>,
}

fn layout(program: &Program) -> Result<Layout, ProgramDiagnostic> {
    let error = |phase, entry, symbol, message| program.error(phase, entry, symbol, message);
    let mut names = BTreeSet::new();
    for (i, symbol) in program.symbols.iter().enumerate() {
        let name = &symbol.name;
        let mut bytes = name.bytes();
        let valid = name != "."
            && bytes
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b == b'.')
            && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.');
        if !valid || !names.insert(name) {
            return Err(error(
                Phase::Validate,
                None,
                Some(SymbolId(i)),
                "invalid or duplicate symbol name".to_owned(),
            ));
        }
    }
    let mut object = Object {
        sections: [vec![], vec![], vec![]],
        alignments: [16; 3],
        include_data: false,
        symbols: vec![],
        relocations: vec![],
    };
    let mut current = None;
    let mut entries = Vec::new();
    let mut symbols = program
        .symbols
        .iter()
        .enumerate()
        .map(|(i, _)| SymbolLocation {
            symbol: SymbolId(i),
            position: None,
        })
        .collect::<Vec<_>>();
    let mut pending = Vec::<(EntryId, SymbolId, bool, Fixup)>::new();
    for (i, entry) in program.entries.iter().enumerate() {
        let id = EntryId(i);
        let fail = |phase, symbol, message| error(phase, Some(id), symbol, message);
        let origins = &entry.provenance.origins;
        if (!entry.provenance.synthetic && origins.is_empty())
            || origins.iter().collect::<BTreeSet<_>>().len() != origins.len()
        {
            return Err(fail(
                Phase::Validate,
                None,
                "origins must be unique; an empty set requires synthetic".to_owned(),
            ));
        }
        if let EntryKind::Section(section) = entry.kind {
            current = Some(section);
            object.include_data |= section == Section::Data;
            entries.push(EntryLocation {
                entry: id,
                provenance: entry.provenance.clone(),
                range: None,
            });
            continue;
        }
        let section = current.ok_or_else(|| {
            fail(
                Phase::Validate,
                None,
                "select a section before emitting an entry".to_owned(),
            )
        })?;
        let index = section.index();
        let start = object.sections[index].len();
        match &entry.kind {
            EntryKind::Label(symbol) => {
                let declared = program.symbols.get(symbol.0).ok_or_else(|| {
                    fail(
                        Phase::Validate,
                        Some(*symbol),
                        "invalid symbol ID".to_owned(),
                    )
                })?;
                if declared.linkage == Linkage::External || symbols[symbol.0].position.is_some() {
                    return Err(fail(
                        Phase::Validate,
                        Some(*symbol),
                        "external or already defined symbol cannot be defined".to_owned(),
                    ));
                }
                symbols[symbol.0].position = Some(Position {
                    section,
                    offset: start as u64,
                });
            }
            EntryKind::Instruction(instruction) => {
                if section != Section::Text {
                    return Err(fail(
                        Phase::Validate,
                        None,
                        "instruction outside Text".to_owned(),
                    ));
                }
                let mut reference = None;
                for op in &instruction.operands {
                    if let Operand::Symbol(symbol) | Operand::RipRelative(symbol) = op {
                        if program.symbols.get(symbol.0).is_none() {
                            return Err(fail(
                                Phase::Validate,
                                Some(*symbol),
                                "invalid symbol ID".to_owned(),
                            ));
                        }
                        if reference
                            .replace((*symbol, matches!(op, Operand::Symbol(_))))
                            .is_some()
                        {
                            return Err(fail(
                                Phase::Validate,
                                Some(*symbol),
                                "at most one symbol reference per instruction is supported"
                                    .to_owned(),
                            ));
                        }
                    }
                }
                let operands = instruction
                    .operands
                    .iter()
                    .map(|op| op.core(|s| program.symbols[s.0].name.clone()))
                    .collect::<Vec<_>>();
                let (bytes, fixup) =
                    encode::instruction_operands(instruction.opcode.mnemonic(), &operands)
                        .map_err(|message| fail(Phase::Encode, reference.map(|r| r.0), message))?;
                if let Some(mut fixup) = fixup {
                    let (symbol, branch) =
                        reference.expect("encoder fixups require symbolic operands");
                    fixup.offset += start;
                    pending.push((id, symbol, branch, fixup));
                }
                object.sections[index].extend(bytes);
            }
            EntryKind::Data(bytes) => object.sections[index].extend_from_slice(bytes),
            EntryKind::CString(value) => {
                object.sections[index].extend_from_slice(value.as_bytes());
                object.sections[index].push(0);
            }
            EntryKind::Data32(bits) => {
                object.sections[index].extend_from_slice(&bits.to_le_bytes())
            }
            EntryKind::Data64(bits) => {
                object.sections[index].extend_from_slice(&bits.to_le_bytes())
            }
            EntryKind::Align(exponent) => {
                let maximum = if program.target == Target::X86_64PcWindowsMsvc {
                    13
                } else {
                    16
                };
                if *exponent > maximum {
                    return Err(fail(
                        Phase::Validate,
                        None,
                        format!("alignment exponent exceeds target limit {maximum}"),
                    ));
                }
                let alignment = 1usize << exponent;
                object.alignments[index] = object.alignments[index].max(alignment);
                object.sections[index].resize(
                    start.next_multiple_of(alignment),
                    if section == Section::Text { 0x90 } else { 0 },
                );
            }
            EntryKind::Marker => {}
            EntryKind::Section(_) => unreachable!(),
        }
        if object.sections[index].len() > i32::MAX as usize {
            return Err(fail(
                Phase::Encode,
                None,
                "section exceeds encoder size limit".to_owned(),
            ));
        }
        entries.push(EntryLocation {
            entry: id,
            provenance: entry.provenance.clone(),
            range: Some(ByteRange {
                section,
                start: start as u64,
                end: object.sections[index].len() as u64,
            }),
        });
    }
    for (i, symbol) in program.symbols.iter().enumerate() {
        if symbol.linkage != Linkage::External && symbols[i].position.is_none() {
            let reference = pending.iter().find(|(_, s, _, _)| s.0 == i).map(|r| r.0);
            return Err(error(
                Phase::Resolve,
                reference,
                Some(SymbolId(i)),
                "internal symbol has no definition".to_owned(),
            ));
        }
        object.symbols.push(super::Symbol {
            name: symbol.name.clone(),
            global: symbol.linkage != Linkage::Local,
            section: symbols[i].position.map(|p| p.section.index()),
            offset: symbols[i].position.map_or(0, |p| p.offset as usize),
        });
    }
    object
        .symbols
        .sort_by(|a, b| (a.global, &a.name).cmp(&(b.global, &b.name)));
    let mut references = Vec::new();
    for (entry, symbol, branch, mut fixup) in pending {
        let position = symbols[symbol.0].position;
        if branch && position.is_some_and(|p| p.section != Section::Text) {
            return Err(error(
                Phase::Resolve,
                Some(entry),
                Some(symbol),
                "branch target must be Text or external".to_owned(),
            ));
        }
        let kind = if let Some(p) = position.filter(|p| p.section == Section::Text) {
            let displacement =
                i32::try_from(p.offset as i128 + fixup.addend as i128 - fixup.offset as i128)
                    .map_err(|_| {
                        error(
                            Phase::Resolve,
                            Some(entry),
                            Some(symbol),
                            "relative displacement exceeds 32 bits".to_owned(),
                        )
                    })?;
            object.sections[0][fixup.offset..fixup.offset + 4]
                .copy_from_slice(&displacement.to_le_bytes());
            ReferenceKind::ResolvedRelative
        } else {
            // ELF external branches, including tail jumps, use PLT32. This does not
            // change the legacy text parser's relocation policy.
            fixup.call = branch;
            match program.target {
                Target::X86_64UnknownLinuxGnu if branch => ReferenceKind::ElfPlt32,
                Target::X86_64UnknownLinuxGnu => ReferenceKind::ElfPc32,
                Target::X86_64PcWindowsMsvc => ReferenceKind::CoffRel32,
            }
        };
        references.push(Reference {
            entry,
            symbol,
            field: ByteRange {
                section: Section::Text,
                start: fixup.offset as u64,
                end: fixup.offset as u64 + 4,
            },
            addend: fixup.addend,
            kind,
        });
        if kind != ReferenceKind::ResolvedRelative {
            object.relocations.push(fixup);
        }
    }
    Ok(Layout {
        object,
        entries,
        symbols,
        references,
    })
}

/// Encode directly from typed values and return section-relative observation records.
pub fn assemble_program(program: &Program) -> Result<Artifact, ProgramDiagnostic> {
    let layout = layout(program)?;
    let object = format::write(&layout.object, program.target)
        .map_err(|message| program.error(Phase::Object, None, None, message))?;
    Ok(Artifact {
        object,
        entries: layout.entries,
        symbols: layout.symbols,
        references: layout.references,
    })
}

/// Render validated input. No external assembler is called. External assemblers may
/// choose different instruction lengths and padding; the ranges belong to our Object.
pub fn render_asm(program: &Program) -> Result<String, ProgramDiagnostic> {
    // Enforce the same layout/format limits as the Object operation.
    assemble_program(program)?;
    let mut text = String::new();
    for symbol in &program.symbols {
        match symbol.linkage {
            Linkage::External => text.push_str(&format!(".extern {}\n", symbol.name)),
            Linkage::Global => text.push_str(&format!(".globl {}\n", symbol.name)),
            Linkage::Local => {}
        }
    }
    for (i, entry) in program.entries.iter().enumerate() {
        text.push_str(&format!(
            "# entry {i} origins {:?} synthetic {}\n",
            entry.provenance.origins, entry.provenance.synthetic
        ));
        match &entry.kind {
            EntryKind::Section(s) => text.push_str(match s {
                Section::Text => ".text\n",
                Section::Data => ".data\n",
                Section::ReadOnlyData if program.target == Target::X86_64PcWindowsMsvc => {
                    ".section .rdata,\"dr\"\n"
                }
                Section::ReadOnlyData => ".section .rodata\n",
            }),
            EntryKind::Label(s) => text.push_str(&format!("{}:\n", program.symbols[s.0].name)),
            EntryKind::Instruction(instruction) => {
                text.push_str(instruction.opcode.mnemonic());
                if !instruction.operands.is_empty() {
                    text.push(' ');
                    if instruction.opcode == crate::typed::Opcode::Call
                        && !matches!(
                            instruction.operands.as_slice(),
                            [crate::typed::Operand::Symbol(_)]
                        )
                    {
                        text.push('*');
                    }
                    text.push_str(
                        &instruction
                            .operands
                            .iter()
                            .map(|op| {
                                op.asm(|s| {
                                    let symbol = &program.symbols[s.0];
                                    if matches!(op, Operand::Symbol(_))
                                        && matches!(op, Operand::Symbol(_))
                                        && symbol.linkage == Linkage::External
                                        && program.target == Target::X86_64UnknownLinuxGnu
                                    {
                                        format!("{}@PLT", symbol.name)
                                    } else {
                                        symbol.name.clone()
                                    }
                                })
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
                text.push('\n');
            }
            EntryKind::Data(bytes) => {
                for byte in bytes {
                    text.push_str(&format!(".byte {byte}\n"));
                }
            }
            EntryKind::CString(value) => {
                if value.bytes().all(|b| b == 10 || (32..127).contains(&b)) {
                    let escaped = value
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n");
                    text.push_str(&format!(".asciz \"{escaped}\"\n"));
                } else {
                    for byte in value.bytes().chain(std::iter::once(0)) {
                        text.push_str(&format!(".byte {byte}\n"));
                    }
                }
            }
            EntryKind::Data32(bits) => text.push_str(&format!(".long 0x{bits:08X}\n")),
            EntryKind::Data64(bits) => text.push_str(&format!(".quad 0x{bits:016X}\n")),
            EntryKind::Align(n) => text.push_str(&format!(".p2align {n}\n")),
            EntryKind::Marker => {}
        }
    }
    if program.target == Target::X86_64UnknownLinuxGnu {
        text.push_str(".section .note.GNU-stack,\"\",@progbits\n");
    }
    Ok(text)
}

#[cfg(test)]
mod tests;
