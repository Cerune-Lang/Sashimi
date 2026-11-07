# Sashimiの文書

[English](README.en.md)

Sashimiは型付きx86-64命令の符号化、ELF64/COFF Object生成、出自とセクション内位置の対応を担当します。Ceruneの言語意味論、ABI、実行環境とリンク処理は担当しません。

## 設計

設計判断とその制約を記録します。各文書は日英の対訳を維持します。

- [型付きアセンブリと生成位置](design/typed-assembly.ja.md)
- [型付き単一命令API](design/typed-instructions.ja.md)
- [型付きProgram・Object・出自対応](design/typed-program.ja.md)

## リファレンス・実装状況

実装済みの範囲は[README](../README.md)と上記の各文書で確認できます。APIは実験段階で、安定版の公開リファレンス仕様はまだありません。未採用の仕様を実装済みとして扱わないでください。

## 文書の確認

Cerune本家のドキュメント管理と同様に、以下をCIで検証します。

```sh
node scripts/check-docs.cjs
node --test scripts/check-docs.test.cjs
```

Git管理対象と未追跡・非除外のMarkdownについて、ローカルリンク先・旧Sashimi URL・日英ペアを検査します。公開文書は `.ja.md` / `.en.md`、READMEは `README.md` / `README.en.md` を使います。GitHub用の `SECURITY.md` は言語ペア検査の対象外です。

外部URLの到達性、見出しアンカー、翻訳内容の一致、実装状況の正確性は人が確認します。これは完全なMarkdown構文検証ではありません。
