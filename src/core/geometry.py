"""
幾何データ構造

ガーバーファイル・ドリルファイルから読み込んだ幾何要素を表現するクラス群
"""

from dataclasses import dataclass
from typing import List, Tuple
from enum import Enum


class GeometryType(Enum):
    """幾何要素の種類"""
    POINT = "point"
    LINE = "line"
    ARC = "arc"
    CIRCLE = "circle"
    POLYGON = "polygon"


@dataclass
class Point:
    """2D/3D点"""
    x: float
    y: float
    z: float = 0.0  # デフォルトは0（2D互換）
    
    def __repr__(self):
        if self.z == 0.0:
            return f"Point({self.x:.3f}, {self.y:.3f})"
        return f"Point({self.x:.3f}, {self.y:.3f}, {self.z:.3f})"


@dataclass
class Line:
    """線分"""
    start: Point
    end: Point
    
    def __repr__(self):
        return f"Line({self.start} -> {self.end})"


@dataclass
class Arc:
    """円弧"""
    start: Point
    end: Point
    center: Point
    clockwise: bool  # True: 時計回り, False: 反時計回り
    
    def __repr__(self):
        direction = "CW" if self.clockwise else "CCW"
        return f"Arc({self.start} -> {self.end}, center={self.center}, {direction})"


@dataclass
class Circle:
    """円"""
    center: Point
    radius: float
    
    def __repr__(self):
        return f"Circle(center={self.center}, radius={self.radius:.3f})"


@dataclass
class Polygon:
    """ポリゴン（閉じた図形）"""
    points: List[Point]
    
    def __repr__(self):
        return f"Polygon({len(self.points)} points)"


@dataclass
class Geometry:
    """
    幾何要素のコンテナ
    
    ガーバーファイルやドリルファイルから読み込んだすべての幾何要素を保持
    """
    lines: List[Line]
    arcs: List[Arc]
    circles: List[Circle]
    polygons: List[Polygon]
    
    def __init__(self):
        self.lines = []
        self.arcs = []
        self.circles = []
        self.polygons = []
    
    def add_line(self, start: Point, end: Point):
        """線分を追加"""
        self.lines.append(Line(start, end))
    
    def add_arc(self, start: Point, end: Point, center: Point, clockwise: bool):
        """円弧を追加"""
        self.arcs.append(Arc(start, end, center, clockwise))
    
    def add_circle(self, center: Point, radius: float):
        """円を追加"""
        self.circles.append(Circle(center, radius))
    
    def add_polygon(self, points: List[Point]):
        """ポリゴンを追加"""
        self.polygons.append(Polygon(points))
    
    def get_bounds(self) -> Tuple[Point, Point]:
        """
        すべての幾何要素を含むバウンディングボックスを取得
        
        Returns:
            (min_point, max_point): 左下と右上の座標
        """
        all_points = []
        
        # 線分から点を収集
        for line in self.lines:
            all_points.extend([line.start, line.end])
        
        # 円弧から点を収集
        for arc in self.arcs:
            all_points.extend([arc.start, arc.end, arc.center])
        
        # 円から点を収集
        for circle in self.circles:
            all_points.append(Point(circle.center.x - circle.radius, circle.center.y - circle.radius))
            all_points.append(Point(circle.center.x + circle.radius, circle.center.y + circle.radius))
        
        # ポリゴンから点を収集
        for polygon in self.polygons:
            all_points.extend(polygon.points)
        
        if not all_points:
            return Point(0, 0), Point(0, 0)
        
        min_x = min(p.x for p in all_points)
        max_x = max(p.x for p in all_points)
        min_y = min(p.y for p in all_points)
        max_y = max(p.y for p in all_points)
        
        return Point(min_x, min_y), Point(max_x, max_y)
    
    def __repr__(self):
        return (f"Geometry(lines={len(self.lines)}, arcs={len(self.arcs)}, "
                f"circles={len(self.circles)}, polygons={len(self.polygons)})")


@dataclass
class DrillHole:
    """ドリル穴"""
    position: Point
    diameter: float
    
    def __repr__(self):
        return f"DrillHole(pos={self.position}, dia={self.diameter:.3f})"


class DrillData:
    """
    ドリルデータのコンテナ
    """
    def __init__(self):
        self.holes: List[DrillHole] = []
    
    def add_hole(self, position: Point, diameter: float):
        """穴を追加"""
        self.holes.append(DrillHole(position, diameter))
    
    def get_bounds(self) -> Tuple[Point, Point]:
        """
        すべての穴を含むバウンディングボックスを取得
        
        Returns:
            (min_point, max_point): 左下と右上の座標
        """
        if not self.holes:
            return Point(0, 0), Point(0, 0)
        
        positions = [hole.position for hole in self.holes]
        min_x = min(p.x for p in positions)
        max_x = max(p.x for p in positions)
        min_y = min(p.y for p in positions)
        max_y = max(p.y for p in positions)
        
        return Point(min_x, min_y), Point(max_x, max_y)
    
    def __repr__(self):
        return f"DrillData({len(self.holes)} holes)"
