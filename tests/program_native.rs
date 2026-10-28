//! Opt-in external-tool comparison. CI sets SASHIMI_TEST_CLANG and requires it to work.
#[cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
#[path = "../examples/support/program.rs"]
mod support;

#[test]
#[cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn program_asm_and_object_link_and_execute_with_external_branches() {
    use sashimi_assembler::{Target, program::*};
    use std::process::Command;
    let Some(clang) = std::env::var_os("SASHIMI_TEST_CLANG") else {
        eprintln!("external link/run comparison not run: set SASHIMI_TEST_CLANG (required in CI)");
        return;
    };
    let target = if cfg!(target_os = "windows") {
        Target::X86_64PcWindowsMsvc
    } else {
        Target::X86_64UnknownLinuxGnu
    };
    let dir = std::env::temp_dir().join(format!("sashimi-program-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = support::example_program(target);
    let a = assemble_program(&p).unwrap();
    assert_eq!(
        a.references
            .iter()
            .filter(|r| if cfg!(target_os = "linux") {
                r.kind == ReferenceKind::ElfPlt32
            } else {
                r.kind == ReferenceKind::CoffRel32
                    && p.symbols[r.symbol.0].linkage == Linkage::External
            })
            .count(),
        3
    );
    std::fs::write(dir.join("program.o"), &a.object).unwrap();
    std::fs::write(dir.join("program.s"), render_asm(&p).unwrap()).unwrap();
    for extension in ["o", "s"] {
        let executable = dir.join(format!("program-{extension}.exe"));
        let mut command = Command::new(&clang);
        if cfg!(target_os = "linux") {
            command.args(["-fPIE", "-pie"]);
        }
        let out = command
            .arg(dir.join(format!("program.{extension}")))
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("clang must be available when SASHIMI_TEST_CLANG is set");
        assert!(
            out.status.success(),
            "link {extension}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out = Command::new(executable).output().unwrap();
        assert!(out.status.success(), "run {extension}: {:?}", out);
        assert_eq!(
            String::from_utf8(out.stdout).unwrap().replace("\r\n", "\n"),
            "SASHIMI typed Program\n".repeat(3)
        );
        assert!(out.stderr.is_empty());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn indirect_register_call_executes_from_both_artifacts() {
    use sashimi_assembler::{Target, program::*, typed::*};
    let Some(clang) = std::env::var_os("SASHIMI_TEST_CLANG") else {
        eprintln!("indirect-call execution skipped; set SASHIMI_TEST_CLANG");
        return;
    };
    let target = if cfg!(windows) {
        Target::X86_64PcWindowsMsvc
    } else {
        Target::X86_64UnknownLinuxGnu
    };
    let mut p = Program::new(target);
    let main = p.declare("main", Linkage::Global);
    let callee = p.declare("callee", Linkage::Local);
    let r = |n| Operand::from(Register::gpr(n, Width::Qword).unwrap());
    for e in [
        EntryKind::Section(Section::Text),
        EntryKind::Label(main),
        EntryKind::Instruction(Instruction::new(
            Opcode::Sub64,
            [Operand::Immediate(40), r(4)],
        )),
        EntryKind::Instruction(Instruction::new(
            Opcode::Lea64,
            [Operand::RipRelative(callee), r(11)],
        )),
        EntryKind::Instruction(Instruction::new(Opcode::Call, [r(11)])),
        EntryKind::Instruction(Instruction::new(
            Opcode::Add64,
            [Operand::Immediate(40), r(4)],
        )),
        EntryKind::Instruction(Instruction::new(Opcode::Ret, [])),
        EntryKind::Label(callee),
        EntryKind::Instruction(Instruction::new(
            Opcode::Mov64,
            [Operand::Immediate(17), r(0)],
        )),
        EntryKind::Instruction(Instruction::new(Opcode::Ret, [])),
    ] {
        p.push(e, Provenance::synthetic());
    }
    let dir = std::env::temp_dir().join(format!("sashimi-indirect-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("program.o"), assemble_program(&p).unwrap().object).unwrap();
    std::fs::write(dir.join("program.s"), render_asm(&p).unwrap()).unwrap();
    for ext in ["o", "s"] {
        let exe = dir.join(format!("program-{ext}.exe"));
        let out = std::process::Command::new(&clang)
            .arg(dir.join(format!("program.{ext}")))
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            std::process::Command::new(exe)
                .output()
                .unwrap()
                .status
                .code(),
            Some(17)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
