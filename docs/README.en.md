# Sashimi documentation

[日本語](README.md)

Sashimi owns typed x86-64 instruction encoding, ELF64/COFF object generation, and provenance-to-section-position mapping. It does not own Cerune language semantics, ABI, execution, or linking.

## Design

These documents record design decisions and constraints. Each has a Japanese-English translation pair.

- [Typed assembly and generated positions](design/typed-assembly.en.md)
- [Typed single-instruction API](design/typed-instructions.en.md)
- [Typed Program, Object, and provenance mapping](design/typed-program.en.md)

## Reference and implementation status

Use the [README](../README.en.md) and design documents above for implemented capabilities. The API is experimental and has no stable standalone reference specification yet. Proposals must not be described as implemented features.

## Documentation checks

As in Cerune's documentation workflow, CI validates these commands:

```sh
node scripts/check-docs.cjs
node --test scripts/check-docs.test.cjs
```

Tracked and untracked non-ignored Markdown files are checked for valid local links, obsolete Sashimi URLs, and Japanese-English partners. Public guides use `.ja.md` / `.en.md`; READMEs use `README.md` / `README.en.md`. The English GitHub policy `SECURITY.md` is exempt from translation pairing.

External URLs, heading anchors, translation equivalence, and implementation accuracy require human review. This is not a complete Markdown parser.
