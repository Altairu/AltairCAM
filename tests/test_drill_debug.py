"""
ドリルデータの状態を確認するテスト
"""

import sys
import os

# srcディレクトリをパスに追加
project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(project_root, 'src'))

from core.excellon_parser import ExcellonParser

def test_drill_data():
    """ドリルデータを確認"""
    parser = ExcellonParser()
    drill_data = parser.parse_file(r'c:\Users\106no\Documents\GitHub\AltairCAM\sample\SSP4.drl')
    
    print(f"総穴数: {len(drill_data.holes)}")
    print(f"\n最初の10個の穴:")
    for i, hole in enumerate(drill_data.holes[:10]):
        print(f"  {i+1}. 位置: ({hole.position.x:.3f}, {hole.position.y:.3f}), 直径: {hole.diameter:.3f}mm")
    
    # バウンディングボックス
    min_point, max_point = drill_data.get_bounds()
    print(f"\nバウンディングボックス:")
    print(f"  Min: ({min_point.x:.3f}, {min_point.y:.3f})")
    print(f"  Max: ({max_point.x:.3f}, {max_point.y:.3f})")
    print(f"  範囲: {max_point.x - min_point.x:.3f} x {max_point.y - min_point.y:.3f} mm")
    
    # すべての穴の位置を確認
    print(f"\nすべての穴の位置（最初の5個と最後の5個）:")
    for i in range(min(5, len(drill_data.holes))):
        hole = drill_data.holes[i]
        print(f"  穴 {i+1}: ({hole.position.x:.6f}, {hole.position.y:.6f})")
    
    print("  ...")
    
    for i in range(max(0, len(drill_data.holes) - 5), len(drill_data.holes)):
        hole = drill_data.holes[i]
        print(f"  穴 {i+1}: ({hole.position.x:.6f}, {hole.position.y:.6f})")

if __name__ == "__main__":
    test_drill_data()
