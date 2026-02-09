"""
プレビュー機能のテスト
"""

import sys
import os

# srcディレクトリをパスに追加
project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(project_root, 'src'))

import tkinter as tk
from ui.preview import PreviewCanvas
from core.gerber_parser import GerberParser
from core.excellon_parser import ExcellonParser

def test_preview():
    """プレビュー機能をテスト"""
    # テスト用ウィンドウ
    root = tk.Tk()
    root.title("Preview Test")
    root.geometry("1000x800")
    
    # プレビューキャンバスを作成
    canvas = PreviewCanvas(root, width=10, height=8)
    canvas.pack(fill=tk.BOTH, expand=True)
    
    # ファイルを読み込み
    print("ファイルを読み込み中...")
    
    # ドリルファイル
    drill_parser = ExcellonParser()
    drill_data = drill_parser.parse_file(r'c:\Users\106no\Documents\GitHub\AltairCAM\sample\SSP4.drl')
    print(f"ドリル穴: {len(drill_data.holes)}個")
    
    # ガーバーファイル（Edge_Cuts）
    gerber_parser = GerberParser()
    geometry = gerber_parser.parse_file(r'c:\Users\106no\Documents\GitHub\AltairCAM\sample\SSP4-Edge_Cuts.gbr')
    print(f"線分: {len(geometry.lines)}本")
    
    # プレビューを更新
    print("プレビューを更新中...")
    canvas.update_preview(geometry=geometry, drill_data=drill_data)
    print("プレビュー表示完了")
    
    root.mainloop()

if __name__ == "__main__":
    test_preview()
