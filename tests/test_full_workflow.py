"""
サンプルファイルを使った統合テスト
"""

import sys
import os

# srcディレクトリをパスに追加
project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(project_root, 'src'))

from core.gerber_parser import GerberParser
from core.excellon_parser import ExcellonParser
from core.mirror import mirror_geometry, mirror_drill_data, MirrorAxis
from core.toolpath import ToolpathGenerator
from gcode.generator import GCodeGenerator

def test_full_workflow():
    """フルワークフローのテスト"""
    print("=== AltairCAM 統合テスト ===\n")
    
    # 1. ドリルファイルを読み込み
    print("1. ドリルファイルを読み込み中...")
    drill_parser = ExcellonParser()
    drill_data = drill_parser.parse_file(r'c:\Users\106no\Documents\GitHub\AltairCAM\sample\SSP4.drl')
    print(f"   {drill_data}")
    print(f"   穴の数: {len(drill_data.holes)}")
    
    # 2. ガーバーファイルを読み込み（Edge_Cuts）
    print("\n2. ガーバーファイル（Edge_Cuts）を読み込み中...")
    gerber_parser = GerberParser()
    edge_geometry = gerber_parser.parse_file(r'c:\Users\106no\Documents\GitHub\AltairCAM\sample\SSP4-Edge_Cuts.gbr')
    print(f"   {edge_geometry}")
   
    # 3. 反転処理（Y軸で反転）
    print("\n3. Y軸で反転中...")
    mirrored_drill = mirror_drill_data(drill_data, MirrorAxis.Y)
    mirrored_edge = mirror_geometry(edge_geometry, MirrorAxis.Y)
    print(f"   反転完了")
    
    # 4. ツールパス生成
    print("\n4. ツールパス生成中...")
    toolpath_gen = ToolpathGenerator()
    toolpath_gen.tool_diameter = 0.5
    
    # ドリル穴のツールパス
    drill_toolpath = toolpath_gen.generate_drill_toolpath(mirrored_drill)
    print(f"   ドリル穴: {len(drill_toolpath)}個")
    
    # 外形カット（簡易実装）
    edge_toolpath = toolpath_gen.generate_board_cutout(mirrored_edge)
    print(f"   外形線: {len(edge_toolpath)}点")
    
    # 5. Gコード生成
    print("\n5. Gコード生成中...")
    gcode_gen = GCodeGenerator()
    gcode_gen.cut_z = -0.1  # ドリル用
    gcode_gen.feed_rate = 100
    
    all_gcode = []
    all_gcode.extend(gcode_gen.generate_header())
    
    # ドリル穴のGコードを生成
    all_gcode.append("(Drill holes)")
    all_gcode.extend(gcode_gen.generate_drill_path(drill_toolpath))
    
    # まずは外形カットはスキップ（ツールパス生成が未完成のため）
    
    all_gcode.extend(gcode_gen.generate_footer())
    
    print(f"   Gコード行数: {len(all_gcode)}")
    
    # 6. Gコードを保存
    output_file = r'c:\Users\106no\Documents\GitHub\AltairCAM\tests\output_test.nc'
    gcode_gen.save_to_file(all_gcode, output_file)
    print(f"\n6. Gコードを保存しました: {output_file}")
    
    # 最初の10行を表示
    print("\n--- Gコード（最初の15行）---")
    for i, line in enumerate(all_gcode[:15]):
        print(line)
    
    print("\n=== テスト完了 ===")

if __name__ == "__main__":
    test_full_workflow()
