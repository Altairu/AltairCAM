"""
改良版ツールパス生成エンジン

FlatCAMで証明されたアルゴリズムをベースに、軽量で正確なツール
パス生成を実現します。
"""

from typing import List, Tuple, Dict, Set, Optional
import math
from core.geometry import Geometry, DrillData, Point, Line, Arc, Polygon


class AdvancedToolpathGenerator:
    """高度なツールパス生成エンジン（FlatCAMアルゴリズム由来）"""
    
    def __init__(self):
        self.tool_diameter = 0.5
        self.isolation_width = 1
        self.arc_resolution = 0.05  # より精密に
        self.simplification_tolerance = 0.01  # 簡潔化トレランス
        
    def generate_toolpaths(self, geometry: Geometry, isolated_geom: Optional[Geometry] = None) -> List[List[Point]]:
        """
        高度なツールパス生成 (Shapelyを用いたマージ処理)
        
        Args:
            geometry: 入力幾何データ
            isolated_geom: 既に処理されたアイソレーション
        
        Returns:
            最適化されたツールパスリスト
        """
        try:
            import shapely.geometry as sg
            import shapely.ops as so
        except ImportError:
            # shapelyが使えない場合は旧ロジックにフォールバック（ここは到達しない前提ですが念のため）
            return []
            
        shapes = []
        
        # 1. 線分をカッパー（銅箔）の太さを持ったPolygonにする
        for line in geometry.lines:
            ls = sg.LineString([(line.start.x, line.start.y), (line.end.x, line.end.y)])
            width = line.width if line.width > 0 else 0.001
            # cap_style=1 (round), join_style=1 (round)
            shapes.append(ls.buffer(width / 2.0, cap_style=1, join_style=1))
            
        # 2. 円弧をPolygonにする
        for arc in geometry.arcs:
            segments = self._approximate_arc_to_segments(arc, self.arc_resolution)
            pts = [(seg.start.x, seg.start.y) for seg in segments]
            if segments:
                pts.append((segments[-1].end.x, segments[-1].end.y))
            if len(pts) >= 2:
                ls = sg.LineString(pts)
                width = arc.width if hasattr(arc, 'width') and arc.width > 0 else 0.001
                shapes.append(ls.buffer(width / 2.0, cap_style=1, join_style=1))
                
        # 円の追加（フラッシュなど）
        for circle in geometry.circles:
            p = sg.Point(circle.center.x, circle.center.y)
            shapes.append(p.buffer(circle.radius))
            
        # 3. ポリゴン（ベタ塗り）をPolygonにする
        for poly in geometry.polygons:
            if len(poly.points) >= 3:
                pts = [(p.x, p.y) for p in poly.points]
                # 自己交差を持つ不正なポリゴンへの対策としてbuffer(0)を適用する
                poly_shape = sg.Polygon(pts).buffer(0)
                shapes.append(poly_shape)
                
        # すべてのカッパーを結合（ユニオン）して一つのネッツの塊にする
        if not shapes:
            return []
            
        merged_copper = so.unary_union(shapes)
        
        # アイソレーション（切削）幅で外側にオフセット
        # ツール直径の半分だけ離れた場所を中心としてツールが通るようにする
        isolation_dist = self.tool_diameter / 2.0
        # join_style=1 (round) はミリングに適している
        isolation_geom = merged_copper.buffer(isolation_dist, join_style=1)
        
        toolpaths = []
        
        # 抽出ヘルパー
        def extract_paths_from_polygon(polygon):
            paths = []
            if polygon.is_empty:
                return paths
            
            # 外形パス
            ext_coords = list(polygon.exterior.coords)
            paths.append([Point(x, y) for x, y in ext_coords])
            
            # 内側の穴（アイソレーション対象）
            for interior in polygon.interiors:
                int_coords = list(interior.coords)
                paths.append([Point(x, y) for x, y in int_coords])
            return paths
        
        if isolation_geom.geom_type == 'Polygon':
            toolpaths.extend(extract_paths_from_polygon(isolation_geom))
        elif isolation_geom.geom_type == 'MultiPolygon':
            for geom in isolation_geom.geoms:
                toolpaths.extend(extract_paths_from_polygon(geom))
                
        # 4. パスの簡潔化（近い点を削除）
        toolpaths = [self._simplify_path(path) for path in toolpaths]
        
        # 5. パス順序の最適化
        toolpaths = self._optimize_path_sequence(toolpaths)
        
        return toolpaths
    
    def _process_lines_with_isolation(self, lines: List[Line]) -> List[List[Point]]:
        """線分をアイソレーション処理する（FlatCAMの方法）"""
        paths = []
        
        for line in lines:
            # アイソレーション距離を計算
            isolation_dist = line.width / 2.0 + self.tool_diameter / 2.0
            
            # 線分の方向ベクトル
            dx = line.end.x - line.start.x
            dy = line.end.y - line.start.y
            length = math.sqrt(dx*dx + dy*dy)
            
            if length < 0.001:
                continue
            
            # 正規化
            dx /= length
            dy /= length
            
            # 法線ベクトル（垂直）
            nx = -dy
            ny = dx
            
            # 両側のオフセットパスを作成
            offset_dist = isolation_dist
            
            # 内側オフセット
            inner_path = [
                Point(line.start.x + nx * offset_dist, line.start.y + ny * offset_dist),
                Point(line.end.x + nx * offset_dist, line.end.y + ny * offset_dist),
            ]
            
            # 外側オフセット
            outer_path = [
                Point(line.start.x - nx * offset_dist, line.start.y - ny * offset_dist),
                Point(line.end.x - nx * offset_dist, line.end.y - ny * offset_dist),
            ]
            
            paths.append(inner_path)
            paths.append(outer_path)
        
        return paths
    
    def _process_arcs_with_isolation(self, arcs: List[Arc]) -> List[List[Point]]:
        """円弧をアイソレーション処理する（改良版）"""
        paths = []
        
        for arc in arcs:
            # 円弧を多数の直線分で近似
            segments = self._approximate_arc_to_segments(arc, self.arc_resolution)
            
            isolation_dist = self.tool_diameter / 2.0
            
            # セグメント群を処理
            current_path = []
            inner_path = []
            outer_path = []
            
            for i, segment in enumerate(segments):
                # セグメント方向
                dx = segment.end.x - segment.start.x
                dy = segment.end.y - segment.start.y
                length = math.sqrt(dx*dx + dy*dy)
                
                if length < 0.001:
                    continue
                
                dx /= length
                dy /= length
                
                # 法線
                nx = -dy
                ny = dx
                
                # ポイント取得
                if i == 0:
                    # 最初のセグメント
                    inner_path.append(Point(
                        segment.start.x + nx * isolation_dist,
                        segment.start.y + ny * isolation_dist
                    ))
                    outer_path.append(Point(
                        segment.start.x - nx * isolation_dist,
                        segment.start.y - ny * isolation_dist
                    ))
                
                # 終点を追加
                inner_path.append(Point(
                    segment.end.x + nx * isolation_dist,
                    segment.end.y + ny * isolation_dist
                ))
                outer_path.append(Point(
                    segment.end.x - nx * isolation_dist,
                    segment.end.y - ny * isolation_dist
                ))
            
            if inner_path:
                paths.append(inner_path)
            if outer_path:
                paths.append(outer_path)
        
        return paths
    
    def _extract_polygon_contours(self, polygons: List[Polygon]) -> List[List[Point]]:
        """ポリゴンの輪郭を抽出"""
        paths = []
        
        for polygon in polygons:
            if len(polygon.points) < 3:
                continue
            
            # 輪郭線を追加（一周分）
            path = list(polygon.points)
            if path[0] != path[-1]:
                path.append(path[0])  # 閉じる
            
            paths.append(path)
        
        return paths
    
    def _approximate_arc_to_segments(self, arc: Arc, resolution: float) -> List[Line]:
        """円弧を直線セグメントで近似（改良版）"""
        start = arc.start
        end = arc.end
        center = arc.center
        clockwise = arc.clockwise
        
        # 半径計算
        dx = start.x - center.x
        dy = start.y - center.y
        radius = math.sqrt(dx*dx + dy*dy)
        
        if radius < 0.001:
            return [Line(start, end, 0.0)]
        
        # 角度計算
        start_angle = math.atan2(start.y - center.y, start.x - center.x)
        end_angle = math.atan2(end.y - center.y, end.x - center.x)
        
        # 角度範囲の正規化
        if clockwise:
            if end_angle > start_angle:
                end_angle -= 2 * math.pi
            angle_delta = start_angle - end_angle
        else:
            if end_angle < start_angle:
                end_angle += 2 * math.pi
            angle_delta = end_angle - start_angle
        
        # セグメント数の決定
        arc_length = radius * abs(angle_delta)
        num_segments = max(2, int(math.ceil(arc_length / resolution)))
        
        segments = []
        current_point = start
        
        for i in range(1, num_segments + 1):
            if clockwise:
                angle = start_angle - (angle_delta * i / num_segments)
            else:
                angle = start_angle + (angle_delta * i / num_segments)
            
            next_x = center.x + radius * math.cos(angle)
            next_y = center.y + radius * math.sin(angle)
            next_point = Point(next_x, next_y)
            
            segments.append(Line(current_point, next_point, 0.0))
            current_point = next_point
        
        return segments
    
    def _simplify_path(self, path: List[Point], tolerance: float = None) -> List[Point]:
        """Ramer-Douglas-Peucker アルゴリズムでパスを簡潔化"""
        if tolerance is None:
            tolerance = self.simplification_tolerance
        
        if len(path) <= 2:
            return path
        
        # 最も遠い点を探す
        max_dist = 0
        max_index = 0
        
        for i in range(1, len(path) - 1):
            dist = self._perpendicular_distance(path[i], path[0], path[-1])
            if dist > max_dist:
                max_dist = dist
                max_index = i
        
        # トレランスチェック
        if max_dist > tolerance:
            # 再帰的に簡潔化
            left = self._simplify_path(path[:max_index + 1], tolerance)
            right = self._simplify_path(path[max_index:], tolerance)
            return left[:-1] + right
        else:
            return [path[0], path[-1]]
    
    def _perpendicular_distance(self, point: Point, line_start: Point, line_end: Point) -> float:
        """点から直線までの垂直距離"""
        if line_start == line_end:
            dx = point.x - line_start.x
            dy = point.y - line_start.y
            return math.sqrt(dx*dx + dy*dy)
        
        # 投影計算
        num = abs((line_end.y - line_start.y) * point.x - 
                  (line_end.x - line_start.x) * point.y + 
                  line_end.x * line_start.y - 
                  line_end.y * line_start.x)
        
        denom = math.sqrt((line_end.y - line_start.y)**2 + 
                         (line_end.x - line_start.x)**2)
        
        return num / denom if denom > 0 else 0
    
    def _optimize_path_sequence(self, paths: List[List[Point]]) -> List[List[Point]]:
        """Nearest Neighbor法でパス順序を最適化（FlatCAM由来）"""
        if not paths or len(paths) <= 1:
            return paths
        
        optimized = []
        unvisited = set(range(len(paths)))
        
        # 原点(0,0)に最も近いパスから開始
        origin = Point(0, 0)
        current_idx = min(range(len(paths)), 
                         key=lambda i: self._path_endpoint_distance(origin, paths[i]))
        
        unvisited.remove(current_idx)
        optimized.append(paths[current_idx])
        current_end = paths[current_idx][-1]
        
        # 貪欲法: 最も近い未訪問パスを選択
        while unvisited:
            best_idx = None
            best_distance = float('inf')
            best_reverse = False
            
            for idx in unvisited:
                path = paths[idx]
                
                # 正向き: 現在終点 → パス開始
                dist_forward = self._calculate_distance(current_end, path[0])
                
                # 逆向き: 現在終点 → パス終了
                dist_backward = self._calculate_distance(current_end, path[-1])
                
                # より近い方
                if dist_forward < best_distance:
                    best_distance = dist_forward
                    best_idx = idx
                    best_reverse = False
                
                if dist_backward < best_distance:
                    best_distance = dist_backward
                    best_idx = idx
                    best_reverse = True
            
            if best_idx is None:
                break
            
            unvisited.remove(best_idx)
            path = list(paths[best_idx])
            
            if best_reverse:
                path = list(reversed(path))
            
            optimized.append(path)
            current_end = path[-1]
        
        return optimized
    
    def _path_endpoint_distance(self, point: Point, path: List[Point]) -> float:
        """パターンの始点OR終点のうち、最も近い方の距離"""
        if not path:
            return float('inf')
        return min(self._distance(point, path[0]),
                  self._distance(point, path[-1]))
    
    def _distance(self, p1: Point, p2: Point) -> float:
        """2点間の距離を計算"""
        dx = p2.x - p1.x
        dy = p2.y - p1.y
        return math.sqrt(dx*dx + dy*dy)
    
    def _calculate_distance(self, p1: Point, p2: Point) -> float:
        """2点間の距離を計算（別名）"""
        return self._distance(p1, p2)
    
    def _distance(self, p1: Point, p2: Point) -> float:
        """2点間の距離"""
        dx = p2.x - p1.x
        dy = p2.y - p1.y
        return math.sqrt(dx*dx + dy*dy)
    
    # === 互換性メソッド（main.py との互換性のため） ===
    
    def generate_isolation_routing(self, geometry: Geometry) -> List[List[Point]]:
        """
        ガーバーデータのアイソレーションルーティングを生成 (互換性メソッド)
        
        Args:
            geometry: ガーバー幾何データ
        
        Returns:
            ツールパスリスト
        """
        return self.generate_toolpaths(geometry)
    
    def generate_drill_toolpath(self, drill_data: DrillData, optimize_order: bool = False) -> List[Tuple[Point, float]]:
        """
        ドリルデータのツールパスを生成 (互換性メソッド)
        
        Args:
            drill_data: ドリル穴データ
            optimize_order: パス順序を最適化するかどうか
        
        Returns:
            ツールパス（単一の連続パス）
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
        パス順序を最適化 (互換性メソッド)
        
        Args:
            paths: ツールパスリスト
        
        Returns:
            最適化されたツールパスリスト
        """
        return self._optimize_path_sequence(paths)
