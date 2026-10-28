# 型付き単一命令API

[English](typed-instructions.en.md)

[全体設計](typed-assembly.ja.md)の段階1です。単一命令を直接符号化するAPIであり、型付きProgramからObjectを作るAPIではありません。

## 使用例

`cargo run --example typed` で、Rustの値から作った命令のASM表示と機械バイトを確認できます。

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

オペランド順はAT&Tと同じく入力側が先、出力側が後です。汎用レジスタ番号0〜7はRAX/RCX/RDX/RBX/RSP/RBP/RSI/RDI、8〜15はR8〜R15です。Widthで対応する8/32/64bitを選びます。XMMは別の `Register::xmm` で作ります。AHなど上位8bitレジスタは未対応です。

`Memory::new(base, index, displacement)` は64bit汎用レジスタのbase、省略可能な(index, Scale)、i32変位を受け取ります。倍率は1/2/4/8。RSPをindexにする指定や、32bit・XMMのアドレスレジスタは拒否します。R12はindexとして利用できます。

## 対応範囲

| 実装済み | 内容 |
| --- | --- |
| 型 | Opcode、Instruction、Operand、Register、Width、Memory、Scale |
| 82個の参照なしOpcode | mov・整数演算・比較・シフト・setcc・SSE・変換・push/pop・ret/cqto/ud2・int $0x29。[正確な列挙](../../src/typed.rs) |
| 直接符号化 | レジスタ、即値、base/indexメモリ。ASM文字列・パーサーを経由しない |
| ASM表示 | 検証後に表示。入力を変更しない |
| 診断 | 不正なレジスタ番号・幅・形式・即値を拒否し、既存Diagnosticで理由を返す |

call/jmp/jcc・RIP相対・セクション・ラベル・データ・Object・OriginIdは[Program API](typed-program.ja.md)で扱います。単一命令APIはシンボル参照を拒否します。既存の `assemble(&str, Target)` は従来の範囲を維持します。Ceruneの本番生成は、この部分APIへ切り替えません。

単一命令はx86-64 ISAのバイト列であり、OSのABIやObject形式を選びません。そのためTarget引数はありません。Program/Objectは明示Targetを必須にします。戻り値を実行するAPIは設けません。

## 共有処理と検証

Opcodeが選ぶ固定の命令名と型付きオペランドを、既存の検証・符号化関数へ直接渡します。任意の命令文字列を受け取る入口はありません。従来のテキスト入口は解析後に同じ関数へ進みます。ASM表示も検証するため、誤った命令を成功として表示しません。

- 参照なし82Opcodeすべてに有効例を置き、原本と両Object形式で照合。
- 9,200組の命令・オペランド構成で、原本との受理／拒否・符号化結果を比較。
- 12例の独立した期待バイト、REX/SIB・即値・変位の境界、不正な幅、全汎用レジスタ番号を検査。
- 表示・符号化を繰り返しても入力と結果が変わらず、既存ラベル・再配置・診断が保たれることを検査。

原本との一致は共通する不具合がないことの証明ではありません。12例の期待バイトはLLVMアセンブラでも確認しています。旧テキスト入力の外部jmpのPIE制約は残ります。新しい[Program API](typed-program.ja.md)は外部分岐のPLT32を実装・検証しています。

切り出し原本は[固定fixture](../../tests/fixtures/extraction/README.md)に保存しています。SOURCE_SNAPSHOT.jsonのハッシュは変更せず、検証先だけを固定fixtureへ移しました。

callはSymbolへの直接呼出しに加え、64bit汎用レジスタ／メモリへの間接呼出しに対応します。型付きのCallとオペランドからFF /2を生成し、ASMではcallq *%rax等と表示します。OSのABIと呼出し先の有効性は利用側の責務です。
