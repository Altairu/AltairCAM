"""
ツールパス最適化のテスト
"""

import sys
import os

# srcディレクトリをパスに追加
project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(project_root, 'src'))

from core.excellon_parser import ExcellonParser
from core.optimizer import ToolpathOptimizer

def test_optimization():
    """最適化機能をテスト"""
    # ドリルファイルを読み込み
    parser = ExcellonParser()
    drill_data = parser.parse_file(r'c:\Users\106no\Documents\GitHub\AltairCAM\sample\SSP4.drl')
    
    print(f"ドリル穴数: {len(drill_data.holes)}")
    
    # 最適化器を作成
    optimizer = ToolpathOptimizer()
    
    # 最適化前後を比較
    comparison = optimizer.compare_optimization(drill_data)
    
    print(f"\n最適化前の移動距離: {comparison['original_distance']:.2f} mm")
    print(f"最適化後の移動距離: {comparison['optimized_distance']:.2f} mm")
    print(f"改善率: {comparison['improvement_percent']:.1f}%")
    print(f"削減距離: {comparison['original_distance'] - comparison['optimized_distance']:.2f} mm")
    
    # 最適化された順序を表示
    print(f"\n最適化された順序（最初の10個）:")
    for i, idx in enumerate(comparison['optimized_indices'][:10]):
        hole = drill_data.holes[idx]
        print(f"  {i+1}. 穴#{idx+1}: ({hole.position.x:.2f}, {hole.position.y:.2f})")

if __name__ == "__main__":
    test_optimization()
