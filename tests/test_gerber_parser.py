"""
ガーバーパーサーのテスト
"""

import sys
import os

# srcディレクトリをパスに追加
project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(project_root, 'src'))

from core.gerber_parser import GerberParser

def test_edge_cuts():
    """Edge_Cutsファイルをテスト"""
    parser = GerberParser()
    
    # Edge_Cutsファイルを解析
    geometry = parser.parse_file(r'c:\Users\106no\Documents\GitHub\AltairCAM\sample\SSP4-Edge_Cuts.gbr')
    
    print(f"解析結果: {geometry}")
    print(f"線分数: {len(geometry.lines)}")
    print(f"円数: {len(geometry.circles)}")
    
    # 各線分を表示
    for i, line in enumerate(geometry.lines):
        print(f"  Line {i+1}: {line}")
    
    # バウンディングボックスを表示
    min_point, max_point = geometry.get_bounds()
    print(f"\nバウンディングボックス:")
    print(f"  Min: {min_point}")
    print(f"  Max: {max_point}")
    print(f"  サイズ: {max_point.x - min_point.x:.2f} x {max_point.y - min_point.y:.2f} mm")

if __name__ == "__main__":
    test_edge_cuts()
