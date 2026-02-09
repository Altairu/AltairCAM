"""
ツールパス生成

幾何データからCNC加工のツールパスを生成する
"""

from typing import List, Tuple
from core.geometry import Geometry, DrillData, Point, Line


class ToolpathGenerator:
    """ツールパス生成器"""
    
    def __init__(self):
        self.tool_diameter = 0.5  # ツール直径 (mm)
        self.isolation_width = 1  # アイソレーション幅（パス数）
        
    def generate_isolation_routing(self, geometry: Geometry) -> List[List[Point]]:
        """
        アイソレーションルーティングのツールパスを生成
        
        配線の周囲を切削して孤立化する
        
        Args:
            geometry: 幾何データ
        
        Returns:
            ツールパスのリスト（各パスは点のリスト）
        """
        toolpaths = []
        
        # 線分からツールパスを生成
        for line in geometry.lines:
            # 簡易実装: 線分の両側にオフセットした線を生成
            # TODO: 完全な実装では、すべての幾何要素からポリゴンを生成し、
            # そのオフセットを計算する必要がある
            
            offset = self.tool_diameter / 2
            
            # 線分のベクトルを計算
            dx = line.end.x - line.start.x
            dy = line.end.y - line.start.y
            length = (dx**2 + dy**2)**0.5
            
            if length == 0:
                continue
            
            # 正規化
            dx /= length
            dy /= length
            
            # 垂直ベクトル
            perp_x = -dy
            perp_y = dx
            
            # 左側のパス
            left_start = Point(
                line.start.x + perp_x * offset,
                line.start.y + perp_y * offset
            )
            left_end = Point(
                line.end.x + perp_x * offset,
                line.end.y + perp_y * offset
            )
            toolpaths.append([left_start, left_end])
            
            # 右側のパス
            right_start = Point(
                line.start.x - perp_x * offset,
                line.start.y - perp_y * offset
            )
            right_end = Point(
                line.end.x - perp_x * offset,
                line.end.y - perp_y * offset
            )
            toolpaths.append([right_start, right_end])
        
        return toolpaths
    
    def generate_board_cutout(self, geometry: Geometry, margin: float = 0.1) -> List[Point]:
        """
        基板外形カットのツールパスを生成
        
        Args:
            geometry: 幾何データ（外形線）
            margin: 外形からの余白 (mm)
        
        Returns:
            ツールパス（点のリスト）
        """
        # TODO: 完全な実装
        # 外形ポリゴンをmargin分外側にオフセットし、
        # タブ（基板と外枠を繋ぐ部分）を追加する
        
        toolpath = []
        
        # 簡易実装: すべての線分の端点を収集
        for line in geometry.lines:
            toolpath.append(line.start)
            toolpath.append(line.end)
        
        return toolpath
    
    def generate_drill_toolpath(self, drill_data: DrillData, 
                               optimize_order: bool = False) -> List[Tuple[Point, float]]:
        """
        ドリル穴のツールパスを生成
        
        Args:
            drill_data: ドリルデータ
            optimize_order: Trueの場合、穴の順序を最適化
        
        Returns:
            (位置, 直径)のタプルのリスト
        """
        if optimize_order:
            # 最適化機能を使用
            from core.optimizer import ToolpathOptimizer
            optimizer = ToolpathOptimizer()
            optimized_indices, _ = optimizer.optimize_drill_path(drill_data)
            
            # 最適化された順序でリストを生成
            return [(drill_data.holes[idx].position, drill_data.holes[idx].diameter) 
                    for idx in optimized_indices]
        else:
            # ドリルデータをそのまま返す
            return [(hole.position, hole.diameter) for hole in drill_data.holes]
    
    def optimize_path_order(self, paths: List[List[Point]]) -> List[List[Point]]:
        """
        ツールパスの順序を最適化（移動距離を最小化）
        
        Args:
            paths: ツールパスのリスト
        
        Returns:
            最適化されたツールパスのリスト
        """
        # TODO: 巡回セールスマン問題の近似解法を実装
        # 現時点では単純に元の順序を返す
        return paths
