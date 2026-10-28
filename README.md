# Sashimi

**命令の生成からObjectのバイト位置まで追える、Rust製の小さなx86-64アセンブラ。**

[English](README.en.md)

Sashimiは、型付きの命令・シンボル・データから、アセンブリ表示とELF64／COFFオブジェクトを生成するライブラリです。入力に付けた`OriginId`と、生成した命令・データの**セクション内バイト範囲**を対応付けられます。

もともとは[Cerune](https://github.com/Cerune-Lang/Cerune)のNativeコード生成から切り出したものですが、特定の言語のIRやABIには依存しません。外部crateへの依存もありません。

> **開発状況：実験段階・API未安定。** x86-64 Linux（ELF64）とWindows（COFF）を対象とします。汎用GNU assemblerの代替、リンカ、JIT、実行環境ではありません。crates.ioには未公開です。

## できること

- **型付き命令の直接符号化** — `Opcode`、`Register`、`Operand`などから機械語バイト列を生成。ASM文字列の組み立て・再解析は不要です。
- **同じ`Program`からASMとObjectを生成** — 命令、ラベル、シンボル、データ、整列、参照を構造化し、AT&T形式のASM表示とELF64／COFFのObjectを出力します。
- **出自と生成位置の対応** — `OriginId`から命令・データのセクション内範囲を追跡。バイトを生成しない`Marker`、合成された項目、再配置参照も区別できます。
- **明示的なターゲットと診断** — Linux GNU／Windows MSVCのx86-64を指定。無効なオペランド、参照、配置などを検証し、問題の項目や出自を診断に残します。
- **既存ASM入力との互換入口** — Ceruneが使用していた限定的なGNU形式ASMを`assemble(&str, Target)`で処理できます。

```text
呼出し元（命令選択・ABI・runtime・ソース位置を管理）
                         |
             型付きProgram + OriginId
                         |
             +-----------+-----------+
             |                       |
        render_asm()           assemble_program()
             |                       |
         ASM表示             ELF64 / COFF Object
                                     + 出自・シンボル・参照の対応表
```

Sashimiが担当するのは、**命令・オペランドの検証、符号化、配置、再配置とObject出力**です。命令選択や呼出規約の決定、ソース位置と`OriginId`の対応付け、リンク・実行は呼出し元が行います。

## 試してみる

Rust環境でリポジトリを取得し、同梱の例を実行できます。

```sh
git clone https://github.com/Cerune-Lang/Sashimi.git
cd Sashimi

# 型付きの単一命令をASMと機械語バイト列で表示
cargo run --example typed

# 型付きProgramからObject・ASM・出自情報を生成（Linux x86-64）
cargo run --example program -- x86_64-unknown-linux-gnu target/program-linux
```

2つ目の例は`program.o`、`program.s`、`origins.txt`を出力します。LinuxではClangで両生成物をそれぞれリンクし、実行結果を比較できます。

```sh
clang -fPIE -pie target/program-linux/program.o -o target/program-linux/from-object
clang -fPIE -pie target/program-linux/program.s -o target/program-linux/from-asm
./target/program-linux/from-object
./target/program-linux/from-asm
```

Windowsは`x86_64-pc-windows-msvc`を指定し、対応するツールチェーンでリンクしてください。**Sashimi自体にリンク処理やCLIはありません**。上記はリポジトリ同梱のRust exampleです。

### Rustから使う

まだ公開crateではありません。Git依存として組み込めます。再現性が必要な場合は特定コミットへの固定を推奨します。

```toml
[dependencies]
sashimi-assembler = { git = "https://github.com/Cerune-Lang/Sashimi.git" }
```

型付き`Program`で「42を返す」x86-64の命令を作り、**同じ入力からASMとObjectを生成し、機械語の位置を確認する**最小例です。

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

生成されるASMの主要部分は次の2命令です（実際の出力にはシンボルや出自の注釈も付きます）。

```asm
movabsq $42, %rax
retq
```

`movabsq`の機械語は`48 b8 2a 00 00 00 00 00 00 00`（10バイト）。呼出し元が付けた`OriginId(7)`は、この命令の**`.text`セクション内の`[0, 10)`**に対応します。`retq`は合成項目（`synthetic`）として別に記録されます。

`artifact.entries`に入るのは、**Sashimiが生成したObjectのセクション内オフセット**です。Objectファイル内の絶対位置やリンク後アドレスではありません。単一命令だけを扱う[`typed` API](docs/design/typed-instructions.ja.md)と、分岐・再配置を含む[`examples/program.rs`](examples/program.rs)も参照してください。


## 対応範囲と制限

| 項目 | 現状 |
| --- | --- |
| ターゲット | Linux x86-64（ELF64）、Windows x86-64（COFF） |
| 入力 | 型付き命令／`Program`、およびCerune由来の限定的なGNU形式ASM |
| 生成物 | 命令バイト列、AT&T形式ASM、Object、セクション内の出自・シンボル・参照情報 |
| 実装済みの基本要素 | レジスタ・即値・メモリ、分岐・RIP相対参照、Text／ReadOnlyData／Data、整列・再配置 |
| 未対応の例 | 汎用GNU assembler互換、任意セクション、間接call/jmp、GOT/TLS、動的リンク、リンク後アドレスの追跡、安定API、CLI |

対応するOpcodeや各再配置の細部は[型付き命令API](docs/design/typed-instructions.ja.md)と[Program API](docs/design/typed-program.ja.md)を参照してください。

## 検証とCeruneでの利用

Windows／LinuxのCIでは、単体・ドキュメントテストに加え、ASMとObjectを外部ツールでリンクして実行比較します。旧実装から切り出したコードは[固定fixture](tests/fixtures/extraction/README.md)と`SOURCE_SNAPSHOT.json`で原本を保持し、移行時の比較基準にしています。

```sh
SASHIMI_TEST_CLANG=clang cargo test --all-targets
python3 scripts/verify_snapshot.py
```

PowerShellでは先に`$env:SASHIMI_TEST_CLANG = 'clang'`を設定してください。Clangを指定しないと外部リンク・実行比較はスキップされます。

Ceruneは、**NativeのLIRからSashimiの型付き`Program`を構築し、同じ入力からASMとObjectを生成する経路に移行済み**です（[Cerune PR #191](https://github.com/Cerune-Lang/Cerune/pull/191)）。移行では旧生成器とのObject一致やリンク後の動作を比較しています。これは移行前後の同等性確認であり、符号化器全体の正しさの証明ではありません。

## 設計・今後の計画

- [型付き機械命令と生成位置の設計](docs/design/typed-assembly.ja.md)
- [型付き単一命令API](docs/design/typed-instructions.ja.md)
- [Program・Object・出自対応API](docs/design/typed-program.ja.md)
- [Cerune言語への段階的な移植構想（Issue #2）](https://github.com/Cerune-Lang/Sashimi/issues/2)

## ライセンス

このプロジェクトは **MIT OR Apache-2.0** のデュアルライセンスです。詳細は[MITライセンス](LICENSE-MIT)と[Apache-2.0ライセンス](LICENSE-APACHE)を参照してください。
