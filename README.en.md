# Sashimi

**A small Rust x86-64 assembler that tracks where generated instructions and data came from.**

[日本語](README.md)

Sashimi builds assembly text and ELF64/COFF object files from typed instructions, symbols, and data. It can associate caller-provided `OriginId` values with the **section-relative byte ranges** of generated instructions and data.

Originally extracted from [Cerune](https://github.com/Cerune-Lang/Cerune)'s native code generator, Sashimi does not depend on Cerune's IR or ABI. It has no external crate dependencies.

> **Status: experimental, unstable API.** Targets x86-64 Linux (ELF64) and Windows (COFF). It is not a drop-in GNU assembler, linker, JIT, or runtime. It has not been published to crates.io.

## What it does

- **Encode typed instructions directly** — Build machine-code bytes from `Opcode`, `Register`, `Operand`, and related values, without building and reparsing assembly text.
- **Produce ASM and objects from one `Program`** — Represent instructions, labels, symbols, data, alignment, and references structurally; generate AT&T assembly or ELF64/COFF objects.
- **Track origins and generated locations** — Associate `OriginId` values with section-relative ranges; distinguish zero-byte `Marker` entries, synthetic entries, and relocation references.
- **Validate explicit targets and inputs** — Select Linux GNU or Windows MSVC x86-64 explicitly, with diagnostics tied to program entries and origins.
- **Retain the legacy text entry point** — Assemble the limited GNU-style assembly subset originally emitted by Cerune using `assemble(&str, Target)`.

```text
Caller (owns instruction selection, ABI, runtime, source mapping)
                         |
                Typed Program + OriginId
                         |
             +-----------+-----------+
             |                       |
        render_asm()           assemble_program()
             |                       |
       Assembly text           ELF64 / COFF object
                                + entry, symbol, reference locations
```

Sashimi handles **instruction/operand validation, encoding, layout, relocation, and object writing**. The caller chooses instructions and calling conventions, maps source locations to `OriginId`, and handles linking and execution.

## Try it

Clone the repository and run the included Rust examples:

```sh
git clone https://github.com/Cerune-Lang/Sashimi.git
cd Sashimi

# Show a typed instruction as assembly and machine-code bytes
cargo run --example typed

# Generate an object, assembly, and origin records (Linux x86-64)
cargo run --example program -- x86_64-unknown-linux-gnu target/program-linux
```

The second example writes `program.o`, `program.s`, and `origins.txt`. On Linux, you can link and run each generated artifact separately with Clang:

```sh
clang -fPIE -pie target/program-linux/program.o -o target/program-linux/from-object
clang -fPIE -pie target/program-linux/program.s -o target/program-linux/from-asm
./target/program-linux/from-object
./target/program-linux/from-asm
```

For Windows, specify `x86_64-pc-windows-msvc` and link with an appropriate toolchain. **Sashimi itself does not contain a linker or CLI**; these commands run bundled Rust examples.

### Use it from Rust

The crate is not published. You can use the Git repository as a dependency; pin a commit when reproducibility matters.

```toml
[dependencies]
sashimi-assembler = { git = "https://github.com/Cerune-Lang/Sashimi.git" }
```

This minimal typed `Program` returns 42. It generates **ASM and an object from the same input**, then shows where an instruction ended up in the machine-code section.

```rust
use sashimi_assembler::{
    Target,
    program::{assemble_program, render_asm, EntryKind, Linkage, OriginId,
              Program, Provenance, Section},
    typed::{Instruction, Opcode, Operand, Register, Width},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut p = Program::new(Target::X86_64UnknownLinuxGnu);
    let main = p.declare("main", Linkage::Global);
    let rax = Register::gpr(0, Width::Qword)?;

    p.push(EntryKind::Section(Section::Text), Provenance::synthetic());
    p.push(EntryKind::Label(main), Provenance::synthetic());
    p.push(
        EntryKind::Instruction(Instruction::new(
            Opcode::MovAbs64,
            vec![Operand::Immediate(42), rax.into()],
        )),
        Provenance::source(OriginId(7)),
    );
    p.push(
        EntryKind::Instruction(Instruction::new(Opcode::Ret, vec![])),
        Provenance::synthetic(),
    );

    let artifact = assemble_program(&p)?;
    std::fs::write("main.o", &artifact.object)?;
    println!("{}", render_asm(&p)?);

    let tracked = artifact.entries.iter()
        .find(|entry| entry.provenance.origins.contains(&OriginId(7)))
        .expect("the movabs instruction has OriginId(7)");
    println!("{:?}", tracked.range);
    Ok(())
}
```

The main part of the generated assembly has two instructions (the actual output also includes symbol and origin annotations):

```asm
movabsq $42, %rax
retq
```

The `movabsq` instruction encodes to `48 b8 2a 00 00 00 00 00 00 00` (10 bytes). The caller-assigned `OriginId(7)` maps to **`[0, 10)` in the `.text` section**. The `retq` entry is separately marked as synthetic.

`artifact.entries` stores **section-relative offsets in Sashimi's generated object**, not absolute object-file offsets or linked addresses. For more detail, see the [single-instruction `typed` API](docs/design/typed-instructions.en.md) and the full [`examples/program.rs`](examples/program.rs), which includes branches and relocations.


## Scope and limitations

| Area | Current support |
| --- | --- |
| Targets | Linux x86-64 (ELF64); Windows x86-64 (COFF) |
| Input | Typed instructions and `Program`; limited GNU-style ASM subset from Cerune |
| Output | Encoded instruction bytes, AT&T ASM, object files, section-relative origins/symbols/references |
| Core operations | Registers, immediates, memory, branches, RIP-relative references, Text/ReadOnlyData/Data, alignment, relocations |
| Not implemented | General GNU assembler compatibility, arbitrary sections, indirect call/jmp, GOT/TLS, dynamic linking, linked-address mappings, stable API, CLI |

For supported opcodes and relocation details, see the [typed instruction API](docs/design/typed-instructions.en.md) and [Program API](docs/design/typed-program.en.md).

## Testing and use in Cerune

Windows and Linux CI cover unit and documentation tests plus external linking/execution of both ASM and object outputs. A [fixed extraction fixture](tests/fixtures/extraction/README.en.md) and `SOURCE_SNAPSHOT.json` preserve the original code bytes for migration comparisons.

```sh
SASHIMI_TEST_CLANG=clang cargo test --all-targets
python3 scripts/verify_snapshot.py
```

In PowerShell, first set `$env:SASHIMI_TEST_CLANG = 'clang'`. Without Clang configured, external link/run comparisons are skipped.

Cerune **now builds a typed Sashimi `Program` from native LIR and produces ASM and objects from that same program** ([Cerune PR #191](https://github.com/Cerune-Lang/Cerune/pull/191)). The migration compared generated objects and linked execution against the earlier emitter. This establishes migration equivalence, not a proof that the entire encoder is correct.

## Design and roadmap

- [Typed assembly and generated locations](docs/design/typed-assembly.en.md)
- [Typed single-instruction API](docs/design/typed-instructions.en.md)
- [Program/object/provenance API](docs/design/typed-program.en.md)
- [Plan to port parts of Sashimi into Cerune (Issue #2)](https://github.com/Cerune-Lang/Sashimi/issues/2)

## License

This project is dual-licensed under **MIT OR Apache-2.0**. See the [MIT license](LICENSE-MIT) and [Apache-2.0 license](LICENSE-APACHE).
