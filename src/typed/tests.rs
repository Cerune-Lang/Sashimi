use super::*;
use crate::{Target, assemble, extracted};

fn r(number: u8, width: Width) -> Operand {
    Register::gpr(number, width).unwrap().into()
}
fn x(number: u8) -> Operand {
    Register::xmm(number).unwrap().into()
}
fn mem(base: u8, index: Option<(u8, Scale)>, displacement: i32) -> Operand {
    Memory::new(
        Register::gpr(base, Width::Qword).unwrap(),
        index.map(|(n, s)| (Register::gpr(n, Width::Qword).unwrap(), s)),
        displacement,
    )
    .unwrap()
    .into()
}
fn check(opcode: Opcode, operands: Vec<Operand>) -> Vec<u8> {
    let input = Instruction::new(opcode, operands);
    let bytes = encode_instruction(&input).unwrap();
    let text = format!(".text\n{}\n", render_instruction(&input).unwrap());
    let raw = format!(
        ".text\n{}",
        bytes
            .iter()
            .map(|b| format!(".byte {b}\n"))
            .collect::<String>()
    );
    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc] {
        let reference = extracted::assemble(&text, target).unwrap();
        assert_eq!(assemble(&text, target).unwrap(), reference, "{text}");
        assert_eq!(
            assemble(&raw, target).unwrap(),
            reference,
            "typed bytes: {text}"
        );
    }
    bytes
}

#[test]
fn known_bytes_cover_rex_sib_immediates_sse_and_byte_registers() {
    let cases = [
        (Opcode::Ret, vec![], vec![0xc3]),
        (Opcode::Undefined, vec![], vec![0x0f, 0x0b]),
        (Opcode::SignExtendRax, vec![], vec![0x48, 0x99]),
        (Opcode::Push, vec![r(15, Width::Qword)], vec![0x41, 0x57]),
        (Opcode::Pop, vec![r(12, Width::Qword)], vec![0x41, 0x5c]),
        (
            Opcode::Mov64,
            vec![r(0, Width::Qword), r(9, Width::Qword)],
            vec![0x49, 0x89, 0xc1],
        ),
        (
            Opcode::Mov8,
            vec![r(4, Width::Byte), r(7, Width::Byte)],
            vec![0x40, 0x88, 0xe7],
        ),
        (
            Opcode::MovAbs64,
            vec![Operand::Immediate(u64::MAX as i128), r(8, Width::Qword)],
            vec![0x49, 0xb8, 255, 255, 255, 255, 255, 255, 255, 255],
        ),
        (
            Opcode::Mov64,
            vec![mem(13, Some((12, Scale::Four)), -128), r(9, Width::Qword)],
            vec![0x4f, 0x8b, 0x4c, 0xa5, 0x80],
        ),
        (
            Opcode::Mov64,
            vec![mem(5, None, 0), r(0, Width::Qword)],
            vec![0x48, 0x8b, 0x45, 0],
        ),
        (
            Opcode::Addsd,
            vec![x(8), x(9)],
            vec![0xf2, 0x45, 0x0f, 0x58, 0xc8],
        ),
        (
            Opcode::SetNe,
            vec![r(4, Width::Byte)],
            vec![0x40, 0x0f, 0x95, 0xc4],
        ),
    ];
    for (opcode, operands, expected) in cases {
        assert_eq!(check(opcode, operands), expected);
    }
}

fn representative(opcode: Opcode) -> Vec<Operand> {
    let name = opcode.mnemonic();
    match opcode {
        Opcode::Ret | Opcode::Undefined | Opcode::SignExtendRax => vec![],
        Opcode::Push
        | Opcode::Pop
        | Opcode::Neg64
        | Opcode::Mul64
        | Opcode::Div64
        | Opcode::SignedDiv64
        | Opcode::Inc64 => vec![r(9, Width::Qword)],
        Opcode::Interrupt => vec![Operand::Immediate(0x29)],
        Opcode::MovAbs64 => vec![Operand::Immediate(42), r(9, Width::Qword)],
        Opcode::ZeroExtend8To32 => vec![r(8, Width::Byte), r(9, Width::Dword)],
        Opcode::ZeroExtend8To64 => vec![r(8, Width::Byte), r(9, Width::Qword)],
        Opcode::Lea64 => vec![mem(12, Some((9, Scale::Eight)), 127), r(9, Width::Qword)],
        Opcode::ShiftLeft64
        | Opcode::ShiftRight64
        | Opcode::ShiftArithmeticRight64
        | Opcode::BitComplement64 => vec![Operand::Immediate(3), r(9, Width::Qword)],
        Opcode::Cvtsi2ssq | Opcode::Cvtsi2sdq => vec![r(9, Width::Qword), x(10)],
        Opcode::Cvttsd2siq => vec![x(10), r(9, Width::Qword)],
        _ if name.starts_with("set") => vec![r(9, Width::Byte)],
        _ if ["ss", "sd", "ps", "pd"]
            .iter()
            .any(|suffix| name.ends_with(suffix)) =>
        {
            vec![x(8), x(9)]
        }
        _ => {
            let width = if name.ends_with('b') {
                Width::Byte
            } else if name.ends_with('l') {
                Width::Dword
            } else {
                Width::Qword
            };
            vec![r(0, width), r(9, width)]
        }
    }
}
#[test]
fn every_opcode_has_a_valid_case_matching_the_extraction() {
    for &opcode in Opcode::ALL {
        if opcode.mnemonic().starts_with('j') || opcode == Opcode::Call {
            continue;
        }
        check(opcode, representative(opcode));
    }
}

#[test]
fn typed_and_text_validation_agree_for_operand_combinations() {
    let candidates = [
        r(0, Width::Byte),
        r(1, Width::Byte),
        r(9, Width::Dword),
        r(12, Width::Qword),
        x(8),
        mem(4, None, 0),
        mem(13, Some((12, Scale::Two)), i32::MAX),
        Operand::Immediate(0),
        Operand::Immediate(i128::MAX),
    ];
    for &opcode in Opcode::ALL {
        let mut cases = vec![
            vec![],
            vec![
                Operand::Immediate(3),
                r(0, Width::Qword),
                r(9, Width::Qword),
            ],
        ];
        for a in &candidates {
            cases.push(vec![a.clone()]);
            for b in &candidates {
                cases.push(vec![a.clone(), b.clone()]);
            }
        }
        for operands in cases {
            let indirect = Instruction::new(opcode, operands.clone());
            if opcode == Opcode::Call && encode_instruction(&indirect).is_ok() {
                let text = format!(".text\n{}\n", render_instruction(&indirect).unwrap());
                assert!(assemble(&text, Target::X86_64UnknownLinuxGnu).is_ok());
                continue;
            }
            let asm = format!(
                ".text\n{} {}\n",
                opcode.mnemonic(),
                operands
                    .iter()
                    .map(|op| op.asm(|_| unreachable!()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let reference = extracted::assemble(&asm, Target::X86_64UnknownLinuxGnu);
            let input = Instruction::new(opcode, operands);
            match encode_instruction(&input) {
                Ok(_) => {
                    assert!(reference.is_ok(), "{asm}");
                    check(opcode, input.operands);
                }
                Err(_) => {
                    assert!(reference.is_err(), "{asm}");
                    assert!(render_instruction(&input).is_err());
                }
            }
        }
    }
}

#[test]
fn invalid_numbers_widths_and_instruction_forms_fail() {
    assert!(Register::gpr(16, Width::Qword).is_err());
    assert!(Register::xmm(255).is_err());
    let base = Register::gpr(0, Width::Qword).unwrap();
    assert!(Memory::new(Register::xmm(0).unwrap(), None, 0).is_err());
    assert!(Memory::new(Register::gpr(0, Width::Dword).unwrap(), None, 0).is_err());
    assert!(
        Memory::new(
            base,
            Some((Register::gpr(4, Width::Qword).unwrap(), Scale::One)),
            0
        )
        .is_err()
    );
    assert!(
        Memory::new(
            base,
            Some((Register::gpr(1, Width::Byte).unwrap(), Scale::One)),
            0
        )
        .is_err()
    );
    for (opcode, args) in [
        (Opcode::Mov64, vec![r(0, Width::Dword), r(1, Width::Qword)]),
        (Opcode::Mov8, vec![Operand::Immediate(256), mem(4, None, 0)]),
        (
            Opcode::MovAbs64,
            vec![Operand::Immediate(1i128 << 64), r(0, Width::Qword)],
        ),
        (
            Opcode::Add64,
            vec![Operand::Immediate(1i128 << 31), r(0, Width::Qword)],
        ),
        (
            Opcode::ShiftLeft64,
            vec![r(0, Width::Byte), r(0, Width::Qword)],
        ),
        (
            Opcode::ShiftLeft64,
            vec![Operand::Immediate(-1), r(0, Width::Qword)],
        ),
        (Opcode::Addsd, vec![r(0, Width::Qword), x(0)]),
        (Opcode::Mov64, vec![mem(4, None, 0), mem(5, None, 0)]),
        (Opcode::Ret, vec![Operand::Immediate(0); 4]),
    ] {
        let instruction = Instruction::new(opcode, args);
        assert!(encode_instruction(&instruction).is_err(), "{instruction:?}");
        assert!(render_instruction(&instruction).is_err());
    }
}

#[test]
fn range_edges_and_repeated_observation_preserve_input_and_bytes() {
    for n in [i64::MIN as i128, i64::MAX as i128, u64::MAX as i128] {
        check(
            Opcode::MovAbs64,
            vec![Operand::Immediate(n), r(15, Width::Qword)],
        );
    }
    for displacement in [i32::MIN, -129, -128, 0, 127, 128, i32::MAX] {
        check(
            Opcode::Mov64,
            vec![
                mem(13, Some((12, Scale::Eight)), displacement),
                r(15, Width::Qword),
            ],
        );
    }
    for width in [Width::Byte, Width::Dword, Width::Qword] {
        for n in 0..16 {
            let opcode = match width {
                Width::Byte => Opcode::Mov8,
                Width::Dword => Opcode::Mov32,
                Width::Qword => Opcode::Mov64,
            };
            check(opcode, vec![r(n, width), r((n + 1) % 16, width)]);
        }
    }
    let input = Instruction::new(
        Opcode::SignedMul64,
        vec![
            Operand::Immediate(i32::MIN as i128),
            mem(4, None, 16),
            r(15, Width::Qword),
        ],
    );
    let saved = input.clone();
    let bytes = encode_instruction(&input).unwrap();
    for _ in 0..3 {
        assert!(!render_instruction(&input).unwrap().is_empty());
        assert_eq!(encode_instruction(&input).unwrap(), bytes);
        assert_eq!(input, saved);
    }
}

#[test]
fn legacy_symbols_relocations_and_diagnostics_remain_unchanged() {
    for source in [
        ".text\ncallq malloc\nretq\n",
        ".text\nstart:\njmp end\nud2\nend:\nje start\nretq\n",
        ".data\n.p2align 3\nstate:\n.quad 0\n.section .rodata\nmsg:\n.asciz \"ok\"\n.text\nmovq state(%rip), %rax\nleaq msg(%rip), %rcx\ncallq puts\nretq\n",
        ".text\ntestb $0, state(%rip)\n.data\nstate:\n.byte 1\n",
        ".text\ncallq cerune_missing\n",
        ".text\n.Lduplicate:\n.Lduplicate:\n",
        ".text\njmp .Lmissing\n",
        ".text\nint $0x80\n",
        ".text\n.p2align 32\n",
        ".text\nmovq %eax, %rbx\n",
    ] {
        for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc] {
            assert_eq!(
                assemble(source, target),
                extracted::assemble(source, target),
                "{source}"
            );
        }
    }
}

#[test]
fn indirect_calls_preserve_register_widths_bytes_and_assembly() {
    for n in 0..16 {
        let i = Instruction::new(Opcode::Call, [r(n, Width::Qword)]);
        let mut expected = vec![];
        if n >= 8 {
            expected.push(0x41);
        }
        expected.extend([0xff, 0xd0 | (n & 7)]);
        assert_eq!(encode_instruction(&i).unwrap(), expected);
        let asm = render_instruction(&i).unwrap();
        assert!(asm.starts_with("callq *%"));
        for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc] {
            let mut p = crate::program::Program::new(target);
            p.push(
                crate::program::EntryKind::Section(crate::program::Section::Text),
                crate::program::Provenance::synthetic(),
            );
            p.push(
                crate::program::EntryKind::Instruction(i.clone()),
                crate::program::Provenance::synthetic(),
            );
            let text = crate::program::render_asm(&p).unwrap();
            assert!(text.contains(&asm));
            assert_eq!(
                crate::program::assemble_program(&p).unwrap().object,
                assemble(&text, target).unwrap()
            );
        }
    }
    let i = Instruction::new(Opcode::Call, [mem(12, None, 8)]);
    assert_eq!(encode_instruction(&i).unwrap(), [0x41, 0xff, 0x54, 0x24, 8]);
    assert_eq!(render_instruction(&i).unwrap(), "callq *8(%r12)");
    for op in [
        r(0, Width::Byte),
        r(0, Width::Dword),
        x(0),
        Operand::Immediate(0),
    ] {
        assert!(encode_instruction(&Instruction::new(Opcode::Call, [op])).is_err());
    }
    for text in ["callq %rax", "callq *symbol", "callq *%eax"] {
        assert!(assemble(&format!(".text\n{text}\n"), Target::X86_64UnknownLinuxGnu).is_err());
    }
}
