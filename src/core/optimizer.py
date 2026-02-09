"""
ツールパス最適化

巡回セールスマン問題（TSP）の近似解法を使用して、
CNCマシンの移動距離を最小化する
"""

from typing import List, Tuple
import math
from core.geometry import Point, DrillData, Geometry


class ToolpathOptimizer:
    """ツールパス最適化クラス"""
    
    def __init__(self):
        self.start_point = Point(0.0, 0.0)  # デフォルトの開始点（原点）
    
    def optimize_drill_path(self, drill_data: DrillData, 
                           start_point: Point = None) -> Tuple[List[int], float]:
        """
        ドリル穴の加工順序を最適化（Nearest Neighbor法）
        
        Args:
            drill_data: ドリルデータ
            start_point: 開始点（Noneの場合は原点）
        
        Returns:
            (最適化された穴のインデックスリスト, 総移動距離)
        """
        if not drill_data.holes:
            return [], 0.0
        
        if start_point is None:
            start_point = self.start_point
        
        # 穴の数
        n = len(drill_data.holes)
        
        # 訪問済みフラグ
        visited = [False] * n
        
        # 最適化されたパス
        optimized_path = []
        
        # 現在位置
        current_pos = start_point
        
        # 総移動距離
        total_distance = 0.0
        
        # すべての穴を訪問
        for _ in range(n):
            # 最も近い未訪問の穴を探す
            nearest_idx = -1
            nearest_dist = float('inf')
            
            for i in range(n):
                if not visited[i]:
                    dist = self._distance(current_pos, drill_data.holes[i].position)
                    if dist < nearest_dist:
                        nearest_dist = dist
                        nearest_idx = i
            
            # 最も近い穴を訪問
            if nearest_idx != -1:
                visited[nearest_idx] = True
                optimized_path.append(nearest_idx)
                total_distance += nearest_dist
                current_pos = drill_data.holes[nearest_idx].position
        
        return optimized_path, total_distance
    
    def optimize_isolation_routing(self, paths: List[List[Point]], 
                                   start_point: Point = None) -> Tuple[List[int], float]:
        """
        アイソレーションルーティングのパス順序を最適化
        
        Args:
            paths: パスのリスト（各パスは点のリスト）
            start_point: 開始点（Noneの場合は原点）
        
        Returns:
            (最適化されたパスのインデックスリスト, 総移動距離)
        """
        if not paths:
            return [], 0.0
        
        if start_point is None:
            start_point = self.start_point
        
        # パスの数
        n = len(paths)
        
        # 訪問済みフラグ
        visited = [False] * n
        
        # 最適化されたパス順序
        optimized_order = []
        
        # 現在位置
        current_pos = start_point
        
        # 総移動距離
        total_distance = 0.0
        
        # すべてのパスを訪問
        for _ in range(n):
            # 最も近い未訪問のパスを探す
            nearest_idx = -1
            nearest_dist = float('inf')
            
            for i in range(n):
                if not visited[i] and paths[i]:
                    # パスの開始点までの距離
                    dist = self._distance(current_pos, paths[i][0])
                    if dist < nearest_dist:
                        nearest_dist = dist
                        nearest_idx = i
            
            # 最も近いパスを訪問
            if nearest_idx != -1:
                visited[nearest_idx] = True
                optimized_order.append(nearest_idx)
                total_distance += nearest_dist
                
                # パスの終点を次の開始点にする
                if paths[nearest_idx]:
                    current_pos = paths[nearest_idx][-1]
        
        return optimized_order, total_distance
    
    def calculate_path_length(self, points: List[Point]) -> float:
        """
        点のリストの総移動距離を計算
        
        Args:
            points: 点のリスト
        
        Returns:
            総移動距離
        """
        if len(points) < 2:
            return 0.0
        
        total = 0.0
        for i in range(len(points) - 1):
            total += self._distance(points[i], points[i + 1])
        
        return total
    
    def _distance(self, p1: Point, p2: Point) -> float:
        """
        2点間のユークリッド距離を計算
        
        Args:
            p1: 点1
            p2: 点2
        
        Returns:
            距離
        """
        dx = p2.x - p1.x
        dy = p2.y - p1.y
        return math.sqrt(dx * dx + dy * dy)
    
    def compare_optimization(self, drill_data: DrillData) -> dict:
        """
        最適化前後の距離を比較
        
        Args:
            drill_data: ドリルデータ
        
        Returns:
            比較結果の辞書
        """
        if not drill_data.holes:
            return {
                'original_distance': 0.0,
                'optimized_distance': 0.0,
                'improvement_percent': 0.0
            }
        
        # 最適化前の距離（元の順序）
        original_points = [self.start_point] + [hole.position for hole in drill_data.holes]
        original_distance = self.calculate_path_length(original_points)
        
        # 最適化後の距離
        optimized_indices, optimized_distance = self.optimize_drill_path(drill_data)
        optimized_distance += self._distance(self.start_point, drill_data.holes[optimized_indices[0]].position)
        
        # 改善率を計算
        if original_distance > 0:
            improvement_percent = ((original_distance - optimized_distance) / original_distance) * 100
        else:
            improvement_percent = 0.0
        
        return {
            'original_distance': original_distance,
            'optimized_distance': optimized_distance,
            'improvement_percent': improvement_percent,
            'optimized_indices': optimized_indices
        }
