# Extraction reference

[日本語](README.md)

Four original sources and the three existing tests from Sashimi 9bb85b60d0967c4d1f3a2881e6b907fa3269229d. object.rs is named mod.rs and object/*.rs are siblings; file bytes are unchanged.

scripts/verify_snapshot.py checks the original Git blob hashes in SOURCE_SNAPSHOT.json. This folder is excluded from Git line-ending conversion and rustfmt. It compiles only under cfg(test), not in production.

Apply features/fixes to src, never to this reference. Retains MIT OR Apache-2.0 and Copyright (c) 2026 Hokutaka.
