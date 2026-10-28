# 切り出し時の原本

[English](README.en.md)

Sashimi 9bb85b60d0967c4d1f3a2881e6b907fa3269229dの4ソースと既存3テストを保存しています。object.rsをmod.rs、object/*.rsを同じフォルダの*.rsとして置き、バイト内容は維持します。

SOURCE_SNAPSHOT.jsonのGit blobハッシュをscripts/verify_snapshot.pyで検査します。このフォルダはGitの改行変換とrustfmtの対象から外しています。cfg(test)だけで比較基準としてコンパイルし、本番には含めません。

新機能や修正はsrc側へ加え、原本へ複製しません。MIT OR Apache-2.0、Copyright (c) 2026 Hokutakaを維持します。
