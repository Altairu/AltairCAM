"""
ヘルプダイアログ

使い方ガイド、ショートカットキー、バージョン情報を表示
"""

import tkinter as tk
from tkinter import ttk


class HelpDialog:
    """ヘルプダイアログクラス"""
    
    @staticmethod
    def show_usage_guide(parent):
        """使い方ガイドを表示"""
        window = tk.Toplevel(parent)
        window.title("AltairCAM 使い方ガイド")
        window.geometry("800x600")
        
        # タブ作成
        notebook = ttk.Notebook(window)
        notebook.pack(fill=tk.BOTH, expand=True, padx=10, pady=10)
        
        # タブ1: 基本操作
        tab1 = ttk.Frame(notebook)
        notebook.add(tab1, text="基本操作")
        HelpDialog._create_basic_usage_tab(tab1)
        
        # タブ2: ファイル読み込み
        tab2 = ttk.Frame(notebook)
        notebook.add(tab2, text="ファイル読み込み")
        HelpDialog._create_file_loading_tab(tab2)
        
        # タブ3: パラメータ設定
        tab3 = ttk.Frame(notebook)
        notebook.add(tab3, text="パラメータ設定")
        HelpDialog._create_parameter_tab(tab3)
        
        # タブ4: Gコード生成
        tab4 = ttk.Frame(notebook)
        notebook.add(tab4, text="Gコード生成")
        HelpDialog._create_gcode_tab(tab4)
        
        # 閉じるボタン
        close_btn = ttk.Button(window, text="閉じる", command=window.destroy)
        close_btn.pack(pady=10)
    
    @staticmethod
    def _create_basic_usage_tab(parent):
        """基本操作タブの内容を作成"""
        text = tk.Text(parent, wrap=tk.WORD, padx=20, pady=20)
        text.pack(fill=tk.BOTH, expand=True)
        
        content = """
AltairCAM 基本操作ガイド

1. ワークフロー
   ① ファイル読み込み → ② プレビュー確認 → ③ パラメータ調整 → ④ Gコード生成

2. 3つのファイルセクション
   • B_Cu（銅箔層）: アイソレーションルーティング用
   • Edge_Cuts（基板外形）: 基板カット用
   • Drill（ドリル穴）: 穴あけ用

3. プレビュー機能
   • 読み込んだファイルを2Dで表示
   • B_Cu: 青色、Edge_Cuts: 緑色、Drill: 赤色で表示
   • ツール直径に応じた線幅と穴サイズで表示

4. 個別Gコード出力
   • 各ファイルごとに異なるパラメータを設定可能
   • 各ファイルごとに独立したGコードを生成
   • B_Cu、Edge_Cuts、Drillで別々のNCファイルを出力

5. ツールパス最適化（ドリルのみ）
   • チェックボックスをONにすると移動距離を最小化
   • 通常10-60%程度の改善が期待できます
"""
        text.insert('1.0', content)
        text.config(state=tk.DISABLED)
    
    @staticmethod
    def _create_file_loading_tab(parent):
        """ファイル読み込みタブの内容を作成"""
        text = tk.Text(parent, wrap=tk.WORD, padx=20, pady=20)
        text.pack(fill=tk.BOTH, expand=True)
        
        content = """
ファイル読み込み

【対応ファイル形式】
• ガーバーファイル: .gbr, .GBR（RS-274X形式）
• ドリルファイル: .drl, .DRL, .txt（Excellon形式）

【読み込み手順】
1. 各ファイルセクションの「参照...」ボタンをクリック
2. 対応するファイルを選択
3. すべての必要なファイルを選択したら「すべて読み込み」をクリック

【KiCadからのエクスポート】
KiCadでPCBを作成後、以下の手順でファイルをエクスポート：

1. ガーバーファイルのエクスポート
   • PCBエディタ → ファイル → プロット
   • フォーマット: Gerber
   • 含めるレイヤー: B.Cu, Edge.Cuts を選択
   • プロットをクリック

2. ドリルファイルのエクスポート
   • 同じプロットダイアログで「ドリルファイルの生成」をクリック
   • フォーマット: Excellon
   • 単位: ミリメートル（推奨）
   • ドリルファイルを生成をクリック

【ファイルの確認】
• ログウィンドウに読み込み状況が表示されます
• プレビューで正しく読み込まれたか確認できます
"""
        text.insert('1.0', content)
        text.config(state=tk.DISABLED)
    
    @staticmethod
    def _create_parameter_tab(parent):
        """パラメータ設定タブの内容を作成"""
        text = tk.Text(parent, wrap=tk.WORD, padx=20, pady=20)
        text.pack(fill=tk.BOTH, expand=True)
        
        content = """
パラメータ設定ガイド

【各パラメータの説明】

1. ツール直径（mm）
   • 使用するエンドミルやドリルビットの直径
   • B_Cu: 0.1-0.2mm（細いエンドミル）
   • Edge_Cuts: 0.8-2.0mm（基板カット用）
   • Drill: 実際のドリルビット直径

2. 切削深さ（mm）
   • マイナス値で設定（Z軸下方向）
   • B_Cu: -0.05～-0.1mm（銅箔のみ削る）
   • Edge_Cuts: -1.6mm以上（基板厚を貫通）
   • Drill: -1.7mm以上（基板厚+余裕）

3. 送り速度（mm/min）
   • CNCマシンの移動速度
   • 一般的な値: 50-200mm/min
   • 速すぎるとツールが折れる可能性あり
   • 遅すぎると加工時間が長くなる

4. 反転軸
   • なし: 反転しない
   • X軸: 上下反転（水平線を軸に反転）
   • Y軸: 左右反転（垂直線を軸に反転）
   • 裏面加工時はY軸反転を使用

5. ツールパス最適化（ドリルのみ）
   • ON: 穴の加工順序を最適化し、移動距離を最小化
   • OFF: ファイルの順序のまま加工
   • 推奨: ONにして加工時間を短縮

【推奨設定例】

FR-4基板（1.6mm厚）の場合：
• B_Cu: φ0.1mm, -0.05mm, 100mm/min
• Edge_Cuts: φ1.0mm, -1.8mm, 80mm/min
• Drill: φ0.8mm, -1.8mm, 50mm/min
"""
        text.insert('1.0', content)
        text.config(state=tk.DISABLED)
    
    @staticmethod
    def _create_gcode_tab(parent):
        """Gコード生成タブの内容を作成"""
        text = tk.Text(parent, wrap=tk.WORD, padx=20, pady=20)
        text.pack(fill=tk.BOTH, expand=True)
        
        content = """
Gコード生成ガイド

【生成手順】
1. ファイルを読み込み、パラメータを設定
2. 各ファイルセクションの「Gコード生成」ボタンをクリック
3. 保存先とファイル名を指定
4. NCファイル（.nc または .gcode）として保存

【生成されるGコード】
• ヘッダー: 初期化コマンド（G21, G90など）
• ツールパス: 実際の加工動作
• フッター: 終了処理（M30など）

【ログの見方】
ドリル最適化時のログ例：
  最適化前の移動距離: 1087.93 mm
  最適化後の移動距離: 452.34 mm
  改善率: 58.4%

【Gコードの確認】
生成されたGコードは以下のツールで確認できます：
• CAMotics（無料、クロスプラットフォーム）
• NCViewer（オンライン）
• bCNC（Python製、編集も可能）

【加工の順序】
推奨される加工順序：
1. B_Cu（銅箔のアイソレーション）
2. Drill（穴あけ）
3. Edge_Cuts（基板カット）

※ Edge_Cutsは最後に行うことで、加工中の基板が動かないようにします

【注意事項】
• 必ず最初に原点設定（X0 Y0 Z0）を行ってください
• Zプローブを使用して正確な基板表面を設定
• ドライラン（空送り）でパスを確認してから実加工
• ツールの突出し長さに注意
"""
        text.insert('1.0', content)
        text.config(state=tk.DISABLED)
    
    @staticmethod
    def show_shortcuts(parent):
        """ショートカットキー一覧を表示"""
        window = tk.Toplevel(parent)
        window.title("ショートカットキー")
        window.geometry("500x400")
        
        text = tk.Text(window, wrap=tk.WORD, padx=20, pady=20)
        text.pack(fill=tk.BOTH, expand=True)
        
        content = """
ショートカットキー一覧

【現在利用可能なショートカット】

※ 現在、キーボードショートカットは実装されていません。
  将来のバージョンで追加予定です。

【予定されている機能】
• Ctrl+O: ファイルを開く
• Ctrl+R: すべて読み込み
• Ctrl+P: プレビュー更新
• Ctrl+G: Gコード生成
• F1: ヘルプを表示
• F5: プレビュー更新
"""
        text.insert('1.0', content)
        text.config(state=tk.DISABLED)
        
        close_btn = ttk.Button(window, text="閉じる", command=window.destroy)
        close_btn.pack(pady=10)
    
    @staticmethod
    def show_about(parent):
        """バージョン情報を表示"""
        window = tk.Toplevel(parent)
        window.title("AltairCAM について")
        window.geometry("500x400")
        
        # メインフレーム
        main_frame = ttk.Frame(window, padding="20")
        main_frame.pack(fill=tk.BOTH, expand=True)
        
        # ロゴ・タイトル
        title_label = ttk.Label(main_frame, text="AltairCAM", 
                               font=("Arial", 24, "bold"))
        title_label.pack(pady=10)
        
        version_label = ttk.Label(main_frame, text="Version 1.0.0", 
                                 font=("Arial", 12))
        version_label.pack()
        
        # 説明
        desc_text = tk.Text(main_frame, wrap=tk.WORD, height=15, padx=10, pady=10)
        desc_text.pack(fill=tk.BOTH, expand=True, pady=20)
        
        description = """
PCB CNC加工用CAMソフトウェア

【主な機能】
• ガーバーファイル（RS-274X）の解析
• Excellonドリルファイルの解析
• 3つのレイヤー管理（B_Cu、Edge_Cuts、Drill）
• ツールパス最適化（移動距離の最小化）
• 2Dプレビュー表示
• 反転機能（裏面加工対応）
• 個別Gコード生成

【技術】
• Python 3.x
• Tkinter（GUI）
• Matplotlib（プレビュー）

【開発】
2026年2月

【ライセンス】
MIT License
"""
        desc_text.insert('1.0', description)
        desc_text.config(state=tk.DISABLED)
        
        # 閉じるボタン
        close_btn = ttk.Button(window, text="閉じる", command=window.destroy)
        close_btn.pack(pady=10)
