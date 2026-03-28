"""
ツールパス生成

幾何データからCNC加工のツールパスを生成する
"""

from typing import List, Tuple, Set
import math
from core.geometry import Geometry, DrillData, Point, Line, Arc, Polygon


class ToolpathGenerator:
    """ツールパス生成器"""
    
    def __init__(self):
        self.tool_diameter = 0.5  # ツール直径 (mm)
        self.isolation_width = 1  # アイソレーション幅（パス数）
        self.arc_resolution = 0.1  # 円弧の直線近似精度 (mm)
        
    def _approximate_arc_to_lines(self, arc: Arc, resolution: float = 0.1) -> List[Line]:
        """
        円弧を直線で近似
        
        Args:
            arc: 円弧データ
            resolution: 直線近似の精度（更小さい = より正確）
        
        Returns:
            直線のリスト
        """
        start = arc.start
        end = arc.end
        center = arc.center
        clockwise = arc.clockwise
        
        # 半径を計算
        dx = start.x - center.x
        dy = start.y - center.y
        radius = math.sqrt(dx*dx + dy*dy)
        
        if radius < 0.001:
            # 退化した円弧（半径がほぼ0） - 直線として処理
            return [Line(start, end, 0.0)]
        
        # 開始角度と終了角度を計算
        start_angle = math.atan2(start.y - center.y, start.x - center.x)
        end_angle = math.atan2(end.y - center.y, end.x - center.x)
        
        # 円弧の角度範囲を計算
        if clockwise:
            # 時計回り
            if end_angle > start_angle:
                end_angle -= 2 * math.pi
            angle_delta = start_angle - end_angle
        else:
            # 反時計回り
            if end_angle < start_angle:
                end_angle += 2 * math.pi
            angle_delta = end_angle - start_angle
        
        # 分割数を計算（弧長とresolutionから決定）
        arc_length = radius * abs(angle_delta)
        num_segments = max(2, int(math.ceil(arc_length / resolution)))
        
        # 直線で近似
        lines = []
        current_point = start
        
        for i in range(1, num_segments + 1):
            if clockwise:
                angle = start_angle - (angle_delta * i / num_segments)
            else:
                angle = start_angle + (angle_delta * i / num_segments)
            
            next_x = center.x + radius * math.cos(angle)
            next_y = center.y + radius * math.sin(angle)
            next_point = Point(next_x, next_y)
            
            lines.append(Line(current_point, next_point, 0.0))  # 円弧はwidthを持たない
            current_point = next_point
        
        return lines
    
    def _point_in_polygon(self, point: Point, polygon: Polygon) -> bool:
        """
        点がポリゴン内にあるか判定（レイキャスト法）
        
        Args:
            point: 判定する点
            polygon: ポリゴン
        
        Returns:
            True if inside, False otherwise
        """
        if len(polygon.points) < 3:
            return False
        
        x, y = point.x, point.y
        inside = False
        
        p1x, p1y = polygon.points[-1].x, polygon.points[-1].y
        for p2 in polygon.points:
            p2x, p2y = p2.x, p2.y
            if y > min(p1y, p2y):
                if y <= max(p1y, p2y):
                    if x <= max(p1x, p2x):
                        if p1y != p2y:
                            xinters = (y - p1y) * (p2x - p1x) / (p2y - p1y) + p1x
                        if p1x == p2x or x <= xinters:
                            inside = not inside
            p1x, p1y = p2x, p2y
        
        return inside
    
    def _line_point_distance(self, p1: Point, p2: Point, p: Point) -> float:
        """点から直線までの距離を計算"""
        dx = p2.x - p1.x
        dy = p2.y - p1.y
        length_sq = dx*dx + dy*dy
        
        if length_sq < 0.0001:
            # 始点と終点がほぼ同じ
            return math.sqrt((p.x - p1.x)**2 + (p.y - p1.y)**2)
        
        t = max(0, min(1, ((p.x - p1.x) * dx + (p.y - p1.y) * dy) / length_sq))
        proj_x = p1.x + t * dx
        proj_y = p1.y + t * dy
        
        return math.sqrt((p.x - proj_x)**2 + (p.y - proj_y)**2)
    
    def _line_intersects_polygon(self, line: Line, polygon: Polygon, margin: float = 0.1) -> bool:
        """
        直線がポリゴン内部と交差しているか判定
        
        Args:
            line: 直線
            polygon: ポリゴン
            margin: マージン (mm)
        
        Returns:
            True if intersects, False otherwise
        """
        # 始点と終点 がポリゴン内にあるか
        if self._point_in_polygon(line.start, polygon) or self._point_in_polygon(line.end, polygon):
            return True
        
        # ポリゴンの各辺との距離をチェック
        for i in range(len(polygon.points)):
            p1 = polygon.points[i]
            p2 = polygon.points[(i + 1) % len(polygon.points)]
            
            dist = self._line_point_distance(line.start, line.end, p1)
            if dist < margin:
                return True
            dist = self._line_point_distance(line.start, line.end, p2)
            if dist < margin:
                return True
        
        return False
    
    def generate_isolation_routing(self, geometry: Geometry) -> List[List[Point]]:
        """
        アイソレーションルーティングのツールパスを生成
        
        Shapelyを利用して全ての銅箔要素（線、円弧、ポリゴン）をユニオンし、
        結合された輪郭からツールパスを生成します。

        Args:
            geometry: 幾何データ

        Returns:
            ツールパスのリスト（各パスは点のリスト）
        """
        try:
            import shapely.geometry as sg
            import shapely.ops as so
        except ImportError:
            return []
            
        shapes = []
        
        # 1. 線分をPolygonにする
        for line in geometry.lines:
            ls = sg.LineString([(line.start.x, line.start.y), (line.end.x, line.end.y)])
            width = line.width if line.width > 0 else 0.001
            shapes.append(ls.buffer(width / 2.0, cap_style=1, join_style=1))
            
        # 2. 円弧をPolygonにする
        for arc in geometry.arcs:
            lines = self._approximate_arc_to_lines(arc, self.arc_resolution)
            pts = [(line.start.x, line.start.y) for line in lines]
            if lines:
                pts.append((lines[-1].end.x, lines[-1].end.y))
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
                poly_shape = sg.Polygon(pts).buffer(0)
                shapes.append(poly_shape)
                
        if not shapes:
            return []
            
        merged_copper = so.unary_union(shapes)
        
        isolation_dist = self.tool_diameter / 2.0
        isolation_geom = merged_copper.buffer(isolation_dist, join_style=1)
        
        toolpaths = []
        
        def extract_paths_from_polygon(polygon):
            paths = []
            if polygon.is_empty:
                return paths
            ext_coords = list(polygon.exterior.coords)
            paths.append([Point(x, y) for x, y in ext_coords])
            for interior in polygon.interiors:
                int_coords = list(interior.coords)
                paths.append([Point(x, y) for x, y in int_coords])
            return paths
        
        if isolation_geom.geom_type == 'Polygon':
            toolpaths.extend(extract_paths_from_polygon(isolation_geom))
        elif isolation_geom.geom_type == 'MultiPolygon':
            for geom in isolation_geom.geoms:
                toolpaths.extend(extract_paths_from_polygon(geom))
                
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
        
        # 円弧も含める
        for arc in geometry.arcs:
            lines = self._approximate_arc_to_lines(arc, self.arc_resolution)
            for line in lines:
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
        
        Nearest Neighbor法で最適な加工順序を決定。
        原点(0,0)から最初のパスが最も近いものから開始。
        
        Args:
            paths: ツールパスのリスト（各パスは点のリスト）
        
        Returns:
            最適化されたツールパスのリスト
        """
        if not paths:
            return []
        
        if len(paths) == 1:
            return paths
        
        # パスの開始点・終了点を取得
        path_info = []
        for i, path in enumerate(paths):
            if len(path) > 0:
                start = path[0]
                end = path[-1]
                path_info.append({
                    'index': i,
                    'start': start,
                    'end': end,
                    'path': path
                })
        
        if not path_info:
            return paths
        
        # 最初の位置を決定（原点0,0から最も近いパスの終点）
        origin = Point(0, 0)
        start_idx = 0
        min_dist = float('inf')
        
        for i, info in enumerate(path_info):
            # 終点から次のパスの開始点までの距離
            dist_to_start = self._calculate_distance(info['end'], info['start'])
            dist_from_origin = self._calculate_distance(origin, info['start'])
            
            if dist_from_origin < min_dist:
                min_dist = dist_from_origin
                start_idx = i
        
        # Nearest Neighbor法で最適順序を決定
        optimized_paths = []
        visited = [False] * len(path_info)
        visited[start_idx] = True
        optimized_paths.append(path_info[start_idx]['path'])
        
        current_end = path_info[start_idx]['end']
        
        while len(optimized_paths) < len(path_info):
            # 次のパスを探す（現在の終点から最も近い始点）
            best_idx = -1
            best_dist = float('inf')
            best_reverse = False
            
            for i, info in enumerate(path_info):
                if visited[i]:
                    continue
                
                # 通常の向き: 現在端点 → パス開始
                dist_forward = self._calculate_distance(current_end, info['start'])
                
                # 逆向き: 現在端点 → パス終了（パスを逆順にする）
                dist_backward = self._calculate_distance(current_end, info['end'])
                
                # より近い方を選択
                if dist_forward <= dist_backward:
                    if dist_forward < best_dist:
                        best_dist = dist_forward
                        best_idx = i
                        best_reverse = False
                else:
                    if dist_backward < best_dist:
                        best_dist = dist_backward
                        best_idx = i
                        best_reverse = True
            
            if best_idx == -1:
                # パスが見つからない場合はスキップ
                for i, info in enumerate(path_info):
                    if not visited[i]:
                        best_idx = i
                        break
            
            if best_idx == -1:
                break
            
            visited[best_idx] = True
            path = path_info[best_idx]['path']
            
            # 逆向きの場合はパスを反転
            if best_reverse:
                path = list(reversed(path))
            
            optimized_paths.append(path)
            current_end = path[-1]
        
        return optimized_paths
    
    def _calculate_distance(self, p1: Point, p2: Point) -> float:
        """2点間の距離を計算"""
        dx = p2.x - p1.x
        dy = p2.y - p1.y
        return math.sqrt(dx*dx + dy*dy)

