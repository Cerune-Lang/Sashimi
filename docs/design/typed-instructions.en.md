# Typed single-instruction API

[日本語](typed-instructions.ja.md)

Stage 1 of the [overall design](typed-assembly.en.md). This directly encodes one instruction; it does not produce objects from a typed Program.

## Example

Run `cargo run --example typed` to display assembly and machine bytes constructed from Rust values.

```rust
use sashimi_assembler::typed::{
    Instruction, Opcode, Operand, Register, Width,
    encode_instruction, render_instruction,
};
let rax = Register::gpr(0, Width::Qword)?;
let instruction = Instruction::new(
    Opcode::MovAbs64, vec![Operand::Immediate(42), rax.into()],
);
assert_eq!(render_instruction(&instruction)?, "movabsq $42, %rax");
assert_eq!(encode_instruction(&instruction)?,
    vec![0x48, 0xb8, 42, 0, 0, 0, 0, 0, 0, 0]);
# Ok::<(), sashimi_assembler::Diagnostic>(())
```

Operands follow AT&T source-before-destination order. GPR numbers 0–7 represent RAX/RCX/RDX/RBX/RSP/RBP/RSI/RDI; 8–15 represent R8–R15. Width selects 8/32/64 bits. Use `Register::xmm` for vector registers. High-byte registers such as AH are unsupported.

`Memory::new(base, index, displacement)` accepts a 64-bit GPR base, optional (index, Scale), and i32 displacement. Scale is 1/2/4/8. RSP as index and 32-bit/XMM addressing registers are rejected. R12 is a valid index.

## Coverage

| Implemented | Details |
| --- | --- |
| Types | Opcode, Instruction, Operand, Register, Width, Memory, Scale |
| 82 reference-free Opcodes | Moves, integer arithmetic, comparisons, shifts, setcc, SSE, conversions, push/pop, ret/cqto/ud2 and int $0x29. [Exact enumeration](../../src/typed.rs) |
| Direct encoding | Registers, immediates, base/index memory; no assembly text or parser |
| Assembly rendering | Validate before rendering, without mutating input |
| Diagnostics | Reject invalid register numbers, widths, forms and immediates using the existing Diagnostic |

Calls/jumps/conditional branches, RIP-relative symbols, sections, labels, data, objects and OriginId are handled by the [Program API](typed-program.en.md). The single-instruction API rejects symbol references. Existing `assemble(&str, Target)` retains its coverage. Cerune production is not switched to this partial API.

An isolated x86-64 instruction has no OS ABI or object format, so this API has no Target argument. Program/object operations require an explicit Target. There is no API to execute the returned bytes.

## Shared implementation and validation

Pass the fixed mnemonic selected by Opcode and typed operands directly to existing validation/encoding functions. There is no arbitrary mnemonic-string input. The legacy text entry point parses first, then uses the same functions. Rendering validates as well, so malformed instructions are not successfully displayed.

- Every reference-free Opcode has a valid case compared with the extraction in both object formats.
- 9,200 instruction/operand combinations compare acceptance, rejection and encoding.
- Twelve independent byte expectations, REX/SIB and immediate/displacement boundaries, invalid widths and every GPR number are tested.
- Repeated rendering/encoding preserves input and output; existing labels, relocations and diagnostics remain unchanged.

Agreement with the extraction cannot rule out shared bugs. The twelve byte expectations were also checked with LLVM's assembler. The legacy text API retains its external-jump PIE limitation. The [Program API](typed-program.en.md) implements and tests PLT32 for external branches.

Original sources live in [fixed fixtures](../../tests/fixtures/extraction/README.en.md). SOURCE_SNAPSHOT.json hashes are unchanged; verification now targets the fixtures.

Call supports both direct Symbol calls and indirect calls through 64-bit GPRs/memory. Typed Call operands encode FF /2 and render as callq *%rax, for example. The caller remains responsible for the OS ABI and target validity.
