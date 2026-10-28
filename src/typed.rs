//! Typed x86-64 instructions and Program-local symbol operands. No assembly parser is used.
//!
//! ```
//! use sashimi_assembler::typed::{
//!     Instruction, Opcode, Operand, Register, Width,
//!     encode_instruction, render_instruction,
//! };
//! let rax = Register::gpr(0, Width::Qword)?;
//! let instruction = Instruction::new(
//!     Opcode::MovAbs64, vec![Operand::Immediate(42), rax.into()],
//! );
//! assert_eq!(render_instruction(&instruction)?, "movabsq $42, %rax");
//! assert_eq!(encode_instruction(&instruction)?,
//!     vec![0x48, 0xb8, 42, 0, 0, 0, 0, 0, 0, 0]);
//! # Ok::<(), sashimi_assembler::Diagnostic>(())
//!
//! ```
use crate::{Diagnostic, object::encode, program::SymbolId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Byte,
    Dword,
    Qword,
}
impl Width {
    fn bits(self) -> u8 {
        match self {
            Self::Byte => 8,
            Self::Dword => 32,
            Self::Qword => 64,
        }
    }
}

/// Constructed registers cannot contain invalid numbers or widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Register {
    number: u8,
    width: u8,
    vector: bool,
}
impl Register {
    pub fn gpr(number: u8, width: Width) -> Result<Self, Diagnostic> {
        Self::new(number, width.bits(), false)
    }
    pub fn xmm(number: u8) -> Result<Self, Diagnostic> {
        Self::new(number, 128, true)
    }
    fn new(number: u8, width: u8, vector: bool) -> Result<Self, Diagnostic> {
        if number >= 16 {
            return Err(Diagnostic::without_span("register number must be below 16"));
        }
        Ok(Self {
            number,
            width,
            vector,
        })
    }
    fn core(self) -> encode::Register {
        encode::Register {
            code: self.number,
            width: self.width,
            vector: self.vector,
        }
    }
    fn asm(self) -> String {
        if self.vector {
            return format!("%xmm{}", self.number);
        }
        if self.number >= 8 {
            return format!(
                "%r{}{}",
                self.number,
                match self.width {
                    8 => "b",
                    32 => "d",
                    _ => "",
                }
            );
        }
        let names = match self.width {
            8 => ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil"],
            32 => ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"],
            _ => ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"],
        };
        format!("%{}", names[self.number as usize])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    One,
    Two,
    Four,
    Eight,
}
impl Scale {
    fn bits(self) -> u8 {
        match self {
            Self::One => 0,
            Self::Two => 1,
            Self::Four => 2,
            Self::Eight => 3,
        }
    }
}

/// Checked base/index addressing. See Operand::RipRelative for symbolic addressing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    base: Register,
    index: Option<(Register, Scale)>,
    displacement: i32,
}
impl Memory {
    pub fn new(
        base: Register,
        index: Option<(Register, Scale)>,
        displacement: i32,
    ) -> Result<Self, Diagnostic> {
        if base.vector
            || base.width != 64
            || index.is_some_and(|(r, _)| r.vector || r.width != 64 || r.number == 4)
        {
            return Err(Diagnostic::without_span(
                "address requires 64-bit GPRs; RSP cannot be an index",
            ));
        }
        Ok(Self {
            base,
            index,
            displacement,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    Register(Register),
    Immediate(i128),
    Memory(Memory),
    Symbol(SymbolId),
    RipRelative(SymbolId),
}
impl From<Register> for Operand {
    fn from(r: Register) -> Self {
        Self::Register(r)
    }
}
impl From<Memory> for Operand {
    fn from(m: Memory) -> Self {
        Self::Memory(m)
    }
}
impl Operand {
    pub(crate) fn core(&self, name: impl Fn(SymbolId) -> String) -> encode::Operand {
        match self {
            Self::Symbol(id) => encode::Operand::Label(name(*id)),
            Self::RipRelative(id) => encode::Operand::Relative(name(*id)),
            Self::Register(r) => encode::Operand::Register(r.core()),
            Self::Immediate(n) => encode::Operand::Immediate(*n),
            Self::Memory(m) => encode::Operand::Memory {
                base: m.base.core(),
                index: m.index.map(|(r, _)| r.core()),
                scale: m.index.map_or(0, |(_, s)| s.bits()),
                displacement: m.displacement,
            },
        }
    }
    pub(crate) fn asm(&self, name: impl Fn(SymbolId) -> String) -> String {
        match self {
            Self::Symbol(id) => name(*id),
            Self::RipRelative(id) => format!("{}(%rip)", name(*id)),
            Self::Register(r) => r.asm(),
            Self::Immediate(n) => format!("${n}"),
            Self::Memory(m) => match m.index {
                Some((r, s)) => format!(
                    "{}({},{},{})",
                    m.displacement,
                    m.base.asm(),
                    r.asm(),
                    1u8 << s.bits()
                ),
                None => format!("{}({})", m.displacement, m.base.asm()),
            },
        }
    }
}

macro_rules! opcodes {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Opcode { $($variant),+ }
        impl Opcode {
            pub const fn mnemonic(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
            #[cfg(test)]
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),+];
        }
    };
}
opcodes! {
    Call => "callq",
    Jump => "jmp",
    JumpO => "jo",
    JumpNo => "jno",
    JumpB => "jb",
    JumpAe => "jae",
    JumpE => "je",
    JumpNe => "jne",
    JumpBe => "jbe",
    JumpA => "ja",
    JumpS => "js",
    JumpNs => "jns",
    JumpP => "jp",
    JumpNp => "jnp",
    JumpL => "jl",
    JumpGe => "jge",
    JumpLe => "jle",
    JumpG => "jg",
    Interrupt => "int",
    Ret => "retq",
    SignExtendRax => "cqto",
    Undefined => "ud2",
    Push => "pushq",
    Pop => "popq",
    Mov8 => "movb",
    Mov32 => "movl",
    Mov64 => "movq",
    MovAbs64 => "movabsq",
    ZeroExtend8To32 => "movzbl",
    ZeroExtend8To64 => "movzbq",
    Lea64 => "leaq",
    Add8 => "addb",
    Add32 => "addl",
    Add64 => "addq",
    Or8 => "orb",
    Or32 => "orl",
    Or64 => "orq",
    And8 => "andb",
    And32 => "andl",
    And64 => "andq",
    Sub8 => "subb",
    Sub32 => "subl",
    Sub64 => "subq",
    Xor8 => "xorb",
    Xor32 => "xorl",
    Xor64 => "xorq",
    Cmp8 => "cmpb",
    Cmp32 => "cmpl",
    Cmp64 => "cmpq",
    Test8 => "testb",
    Test32 => "testl",
    Test64 => "testq",
    Neg64 => "negq",
    Mul64 => "mulq",
    Div64 => "divq",
    SignedDiv64 => "idivq",
    Inc64 => "incq",
    SignedMul64 => "imulq",
    ShiftLeft64 => "shlq",
    ShiftRight64 => "shrq",
    ShiftArithmeticRight64 => "sarq",
    BitComplement64 => "btcq",
    MoveIfNotEqual64 => "cmovne",
    Movss => "movss",
    Movsd => "movsd",
    Movaps => "movaps",
    Movapd => "movapd",
    Xorps => "xorps",
    Xorpd => "xorpd",
    Ucomiss => "ucomiss",
    Ucomisd => "ucomisd",
    Addss => "addss",
    Addsd => "addsd",
    Subss => "subss",
    Subsd => "subsd",
    Mulss => "mulss",
    Mulsd => "mulsd",
    Divss => "divss",
    Divsd => "divsd",
    Cvtsi2ssq => "cvtsi2ssq",
    Cvtsi2sdq => "cvtsi2sdq",
    Cvttsd2siq => "cvttsd2siq",
    Cvtss2sd => "cvtss2sd",
    Cvtsd2ss => "cvtsd2ss",
    SetO => "seto",
    SetNo => "setno",
    SetB => "setb",
    SetAe => "setae",
    SetE => "sete",
    SetNe => "setne",
    SetBe => "setbe",
    SetA => "seta",
    SetS => "sets",
    SetNs => "setns",
    SetP => "setp",
    SetNp => "setnp",
    SetL => "setl",
    SetGe => "setge",
    SetLe => "setle",
    SetG => "setg",
}

/// Operands use source-before-destination order, matching AT&T assembly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: Opcode,
    pub operands: Vec<Operand>,
}
impl Instruction {
    pub fn new(opcode: Opcode, operands: impl Into<Vec<Operand>>) -> Self {
        Self {
            opcode,
            operands: operands.into(),
        }
    }
}

/// Encode one relocation-free instruction. The result is machine bytes, not an object.
/// This operation is ISA-specific and independent of the operating system target.
pub fn encode_instruction(instruction: &Instruction) -> Result<Vec<u8>, Diagnostic> {
    if instruction
        .operands
        .iter()
        .any(|op| matches!(op, Operand::Symbol(_) | Operand::RipRelative(_)))
    {
        return Err(Diagnostic::without_span(
            "symbol references require the program API",
        ));
    }
    let operands: Vec<_> = instruction
        .operands
        .iter()
        .map(|op| op.core(|_| unreachable!()))
        .collect();
    let (bytes, fixup) = encode::instruction_operands(instruction.opcode.mnemonic(), &operands)
        .map_err(Diagnostic::without_span)?;
    if fixup.is_some() {
        return Err(Diagnostic::without_span(
            "symbol references require the program API",
        ));
    }
    Ok(bytes)
}

/// Render only validated instructions. This does not execute or reparse the text.
pub fn render_instruction(instruction: &Instruction) -> Result<String, Diagnostic> {
    encode_instruction(instruction)?;
    let args = instruction
        .operands
        .iter()
        .map(|op| {
            let text = op.asm(|_| unreachable!());
            if instruction.opcode == Opcode::Call {
                format!("*{text}")
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(if args.is_empty() {
        instruction.opcode.mnemonic().to_owned()
    } else {
        format!("{} {args}", instruction.opcode.mnemonic())
    })
}

#[cfg(test)]
mod tests;
