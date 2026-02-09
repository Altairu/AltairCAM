# AltairCAM

KiCadで作成したガーバーファイル・ドリルファイルからCNC加工用のGコードを生成するCAMソフトウェア

## 特徴

- **ガーバーファイル対応**: RS-274X形式のガーバーファイルを読み込み
- **ドリルファイル対応**: Excellon形式のドリルファイルを読み込み
- **反転機能**: 基板切削に必須の反転機能を搭載（X軸/Y軸反転）
- **ツールパス生成**: アイソレーションルーティング、外形カット、ドリル加工
- **Gコード出力**: Grbl/USBCNC対応

## インストール

```bash
# リポジトリをクローン
git clone https://github.com/Altairu/AltairCAM.git
cd AltairCAM

# 依存パッケージをインストール
pip install -r requirements.txt
```

## 使い方

```bash
python src/main.py
```

## 開発状況

現在開発中です。

## ライセンス

MIT License
