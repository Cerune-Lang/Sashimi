use sashimi_assembler::{Target, program::*, typed::*};

/// A caller-owned ABI example: Sashimi never chooses argument registers or stack layout.
pub fn example_program(target: Target) -> Program {
    let mut p = Program::new(target);
    let main = p.declare("main", Linkage::Global);
    let msg = p.declare("message", Linkage::Local);
    let puts = p.declare("puts", Linkage::External);
    let tail = p.declare("tail", Linkage::Local);
    let conditional = p.declare("conditional", Linkage::Local);
    let windows = target == Target::X86_64PcWindowsMsvc;
    let arg: Operand = Register::gpr(if windows { 1 } else { 7 }, Width::Qword)
        .unwrap()
        .into();
    let rsp: Operand = Register::gpr(4, Width::Qword).unwrap().into();
    let eax: Operand = Register::gpr(0, Width::Dword).unwrap().into();
    let stack = Operand::Immediate(if windows { 40 } else { 8 });
    let mut emit = |kind| {
        p.push(kind, Provenance::synthetic());
    };
    emit(EntryKind::Section(Section::ReadOnlyData));
    emit(EntryKind::Align(6));
    emit(EntryKind::Label(msg));
    emit(EntryKind::Data(b"Sashimi typed Program\0".to_vec()));
    emit(EntryKind::Section(Section::Text));
    emit(EntryKind::Label(main));
    let mut instruction = |op, args| {
        p.push(
            EntryKind::Instruction(Instruction::new(op, args)),
            Provenance::source(OriginId(1)),
        );
    };
    instruction(Opcode::Sub64, vec![stack.clone(), rsp.clone()]);
    for function in [puts, tail, conditional] {
        instruction(Opcode::Lea64, vec![Operand::RipRelative(msg), arg.clone()]);
        instruction(Opcode::Call, vec![Operand::Symbol(function)]);
    }
    instruction(Opcode::Xor32, vec![eax.clone(), eax]);
    instruction(Opcode::Add64, vec![stack, rsp]);
    instruction(Opcode::Ret, vec![]);
    p.push(EntryKind::Label(tail), Provenance::synthetic());
    p.push(
        EntryKind::Instruction(Instruction::new(Opcode::Jump, vec![Operand::Symbol(puts)])),
        Provenance::source(OriginId(2)),
    );
    p.push(EntryKind::Label(conditional), Provenance::synthetic());
    p.push(
        EntryKind::Instruction(Instruction::new(
            Opcode::Cmp64,
            vec![Operand::Immediate(0), arg],
        )),
        Provenance::source(OriginId(3)),
    );
    p.push(
        EntryKind::Instruction(Instruction::new(
            Opcode::JumpNe,
            vec![Operand::Symbol(puts)],
        )),
        Provenance::source(OriginId(3)),
    );
    p.push(
        EntryKind::Instruction(Instruction::new(Opcode::Ret, vec![])),
        Provenance::synthetic(),
    );
    p
}
