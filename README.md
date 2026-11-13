# AltairCAM

KiCadなどで設計したプリント基板をCNCフライス盤で加工するためのCAMソフトウェアです。ガーバーファイルとExcellonドリルファイルを読み込み、切削用のGコードを出力します。

Rustで実装されており、依存ファイルなしの単体exeとして動作します。GitHub Releasesを使った自動アップデートにも対応しています。

## ダウンロード

GitHubの [Releasesページ](https://github.com/Altairu/AltairCAM/releases) から `AltairCAM-windows-x86_64.exe` をダウンロードしてください。インストール作業は不要で、任意のフォルダに置いてそのまま起動できます。

新しいバージョンが公開されている場合は起動時に通知が出るため、そのままアプリ内から更新できます。

## ビルド方法

手元でビルドする場合はRust環境を用意し、以下を実行します。

```bash
git clone https://github.com/Altairu/AltairCAM.git
cd AltairCAM
cargo build --release
```

完了すると `target/release/AltairCAM.exe` が生成されます。

## 主な機能

* RS-274Xガーバーの読み込みとアパーチャマクロや極性切り替えへの対応
* Excellonドリルの読み込みと長穴スロットへの対応
* 銅箔パターンの結合と複数周回指定可能なアイソレーションパス生成
* 基板外形の外側 / 内側 / 線上のカット位置切り替え
* Z軸の多段切り込みとスピンドル回転数の指定
* ワーク原点合わせと基準点での反転処理
* 2Dプレビュー / 3D表示 / Gコードシミュレーター
* 設定の自動保存とプロジェクト単位での書き出し

## 使い方

* KiCadから裏面銅箔、外形、ドリルの各ファイルを出力します。
* 左側パネルの各タブからファイルを指定し、すべて読み込みを押します。
* 必要に応じて原点合わせやツール径、切り込み深さを設定します。
* 2Dプレビューとシミュレーションでパスを確認します。
* Gコード出力からNCファイルを保存します。

## リリース作成手順

`v` から始まるバージョンタグをプッシュすると、GitHub ActionsがWindows用の実行ファイルをビルドして自動でReleasesに公開します。

```bash
git tag v2.0.1
git push origin v2.0.1
```

## ライセンス

[MIT License](LICENSE) のもとで公開しています。アルゴリズムの参考元については [ATTRIBUTION.md](ATTRIBUTION.md) を参照してください。
