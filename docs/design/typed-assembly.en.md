# Typed machine instructions and generated locations

[日本語](typed-assembly.ja.md)

Stage 1 [single-instruction API](typed-instructions.en.md) and stage 2 [Program/object/provenance](typed-program.en.md) are implemented. Migrating Cerune to typed construction is next. Scope is the existing x86-64 instruction subset and ELF64/COFF. The Cerune-language port in [Sashimi #2](https://github.com/Cerune-Lang/Sashimi/issues/2) is a separate stage.

## Purpose and responsibilities

Accept already selected machine instructions as values and produce assembly, objects and provenance mappings from the same input. Do not accept Cerune's high-level IR or repeat source semantics.

| Owner | Responsibilities |
| --- | --- |
| Cerune or another caller | Language semantics, MIR/LIR, instruction selection, ABI, runtime, mapping source locations to OriginId |
| Sashimi | Operand/instruction validation, encoding, layout, label resolution, relocations, assembly/ELF/COFF output and generated locations |
| Linker and executor | Final addresses, external symbol resolution and execution; never invoked implicitly by Sashimi |

```text
Caller instruction selection → typed Program + origin IDs
                                 ├→ assembly rendering
                                 └→ validation / encoding / layout → object + provenance
```

The two outputs share selected instructions. Observing provenance must not change instructions, evaluation order or failure conditions.

## Information currently supplied by parsing

| Current code | Current work | Destination |
| --- | --- | --- |
| register / operand in encode.rs | Parse register class/width, immediates, addressing and label references | Text frontend; typed input passes validated values |
| validate / encode in encode.rs | Check forms, widths and ranges; encode | Shared core |
| parse.rs | Select sections, data, alignment, labels, visibility and resolve fixups | Typed entry validation and layout |
| format.rs | Write sections, symbols and relocations as ELF/COFF | Shared object output |
| Cerune emit.rs and runtime helpers | ABI, instruction selection, text and origin labels | Keep selection in Cerune; gradually replace text construction with typed construction |

The current parser rejects unresolved `.L` and `cerune_` names and treats other unresolved references as external symbols. Preserve this rule at the legacy text entry point; do not introduce prefix-based semantics into the new core.

## Typed input

A `Program` contains an explicit Target, symbol table and ordered entries. Initially support Text, ReadOnlyData and Data, without generalizing to arbitrary ISAs or sections.

| Element | Meaning |
| --- | --- |
| Instruction | Closed Opcode enum and typed operands; no arbitrary mnemonic string escape hatch |
| Register / Operand | General 8/32/64-bit and XMM registers, immediates, base/index/scale/displacement, RIP-relative and branch references; validate widths, numbers and scales |
| SymbolId / Symbol | Program-local ID, name, internal definition/external declaration, local/global binding; internal symbols must be defined exactly once |
| Section / Label / Data / Align | Explicit section, symbol definition, byte array, power-of-two alignment; no implicit NUL added to data |
| EntryId | Identifies an input entry in diagnostics and output mappings |
| OriginId / provenance | A set of caller-defined opaque u64 IDs or explicit Synthetic |
| Marker | Zero-byte entry, including operations such as ObserveOnly |

Even if immediates use a wide integer representation, encoding must check the range required by each instruction form. Invalid registers or symbols, duplicate definitions, undefined internal references and out-of-range displacements are errors. Specify a safe grammar for assembly symbol names and reject newlines, NUL and directive injection.

Require external declarations instead of guessing from names. Instruction references produce fixups, which layout resolves or converts into relocations. Initially retain the existing text-section reference forms; arbitrary data pointers and arbitrary relocation input are deferred.

Public operations are listed below; see the [Program API](typed-program.en.md) for the implemented contract.

- `render_asm(&Program)`: validate and render target-specific assembly.
- `assemble_program(&Program)`: produce object and mappings without assembly text.
- `assemble(&str, Target)`: preserve the legacy entry point, gradually parsing into Program to share the core.

Serializing typed input back to text and invoking the existing parser does not complete the migration.

## Provenance mappings

Locations are section-relative byte offsets and half-open ranges `[start, end)`. They are distinct from object-file offsets, assembly line numbers and linked virtual addresses.

| Case | Result |
| --- | --- |
| One origin expands to multiple instructions or disjoint ranges | Preserve multiple EntryIds and ranges; do not merge them implicitly |
| Multiple origins share an instruction | Allow multiple IDs to refer to the same range |
| Marker, empty Data, zero padding | Preserve section and `start == end`; distinguish from no record |
| Prologue, runtime, alignment | Explicit Synthetic; also preserve caller-assigned IDs when supplied |
| Section selection, external declaration | Use EntryId for section selections and SymbolId for external declarations; invent no byte ranges |
| Resolved references and external relocations | Record source EntryId, symbol, displacement field, and resolution/relocation kind |

For example, Origin 7 can map to text ranges `[0, 3)` and `[8, 10)`, Origin 8 can mark `[10, 10)`, and `[3, 8)` can be synthetic padding. IDs are interpreted within one assembly input; identity across Programs is not guaranteed.

Generate objects and mappings together from the same validated input. Disabling observation alone must leave the entire object unchanged. Adding existing origin symbols is a separate operation: symbol tables may change, so compare text/data and relocation meaning instead of demanding identical objects. Mappings provide no API to mutate input or re-encode it.

Do not assign object headers and symbol tables to invented source locations. Initial tracking covers instructions, data, alignment padding and associated symbols/references. A proof accounting for every object-file byte is out of scope.

## Diagnostics and compatibility

Typed diagnostics carry phase, reason, EntryId and relevant OriginIds/SymbolIds. Cerune maps these back to NodeId/SourceId/Span. Sashimi does not fabricate source positions. Preserve `Diagnostic.message` and assembly-line diagnostics at the legacy entry point.

Retain existing text padding with 0x90 and data padding with zero. Do not introduce branch relaxation or instruction reordering. Check that internal SymbolId numbering does not change deterministic object symbol ordering.

## Implementation order and completion criteria

| Stage | Change | Verification |
| --- | --- | --- |
| 1 | Typed values, validation, direct encoding and assembly rendering in Sashimi | Known instruction bytes, invalid inputs, typed/text comparison; grow from explicitly bounded cases |
| 2 | Symbols, sections, data, relocations and provenance | Both targets; disjoint/empty ranges, Synthetic, declarations, padding, references and observation invariance |
| 3 | Migrate Cerune instruction selection and runtime helpers to typed construction | Existing 135 runnable examples, ordinary MIR/SSA, annotations on/off, failure diagnostics, I/O order, external assembler and linked execution |
| 4 | Remove remaining generated-text parsing and retire migration reference | Every normal generation route emits assembly/object from the same Program; retain compatibility input and independent tests |

The goal is all currently supported instruction forms. Document intermediate gaps explicitly and do not switch Cerune production to a less capable route.

Preserve `SOURCE_SNAPSHOT.json` as extraction history. Before modifying implementation files, save the original four files in fixed fixtures and point fidelity verification there. Do not update hashes of changed implementation files and claim they still match the extraction.

## Existing external jumps and PIE linking

An allocation-failure object fixture in Cerune PR #184 used a jump to external malloc. The pinned encoder emitted ELF PC32 and Linux CI rejected the PIE link. Calls use PLT32, but jumps do not receive the same treatment. Normal Cerune output calls external functions; the shared ASM/object fixture is being changed to an ABI-compliant call and tested without disabling PIE.

Typed Programs use PLT32 for external calls, jumps and conditional branches, and explicitly render @PLT in assembly. ASM/Object linking and execution are tested on Windows and Linux with PIE enabled. Internal branches and RIP-relative data references are tested separately. The legacy text entry point retains PC32 external jumps for compatibility.

## Coordinating repositories

Use an independent Sashimi PR for API changes and tests. Pin its published full commit ID in Cerune's Cargo.toml and both Cargo.lock files, and link the Sashimi PR from the Cerune PR. State explicitly when dependencies remain unmerged.

Do not commit sibling-clone path dependencies into shared configuration. Developers must be able to clone only Cerune and fetch its dependency. Document ABI/provenance contract changes in both languages and compare old/new routes before updating the connection.
