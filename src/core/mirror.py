"""
反転（ミラーリング）機能

基板切削では裏面から切削するため、ガーバーデータとドリルデータを反転する必要がある
"""

from enum import Enum
from core.geometry import Geometry, DrillData, Point


class MirrorAxis(Enum):
    """反転軸"""
    X = "x"  # X軸を基準に反転（上下反転）
    Y = "y"  # Y軸を基準に反転（左右反転）


def mirror_point(point: Point, axis: MirrorAxis, reference: float = 0.0) -> Point:
    """
    点を反転する
    
    Args:
        point: 反転する点
        axis: 反転軸（X軸またはY軸）
        reference: 反転の基準位置（デフォルトは0）
    
    Returns:
        反転後の点
    """
    if axis == MirrorAxis.X:
        # X軸を基準に反転（Y座標を反転）
        return Point(point.x, 2 * reference - point.y)
    else:  # MirrorAxis.Y
        # Y軸を基準に反転（X座標を反転）
        return Point(2 * reference - point.x, point.y)


def mirror_geometry(geometry: Geometry, axis: MirrorAxis, reference: float = None) -> Geometry:
    """
    幾何データを反転する
    
    Args:
        geometry: 反転する幾何データ
        axis: 反転軸（X軸またはY軸）
        reference: 反転の基準位置（Noneの場合はバウンディングボックスの中心）
    
    Returns:
        反転後の幾何データ
    """
    # 基準位置が指定されていない場合、バウンディングボックスの中心を使用
    if reference is None:
        min_point, max_point = geometry.get_bounds()
        if axis == MirrorAxis.X:
            reference = (min_point.y + max_point.y) / 2
        else:
            reference = (min_point.x + max_point.x) / 2
    
    mirrored = Geometry()
    
    # 線分を反転
    for line in geometry.lines:
        mirrored_start = mirror_point(line.start, axis, reference)
        mirrored_end = mirror_point(line.end, axis, reference)
        mirrored.add_line(mirrored_start, mirrored_end)
    
    # 円弧を反転
    for arc in geometry.arcs:
        mirrored_start = mirror_point(arc.start, axis, reference)
        mirrored_end = mirror_point(arc.end, axis, reference)
        mirrored_center = mirror_point(arc.center, axis, reference)
        # 反転すると回転方向が逆になる
        mirrored_clockwise = not arc.clockwise
        mirrored.add_arc(mirrored_start, mirrored_end, mirrored_center, mirrored_clockwise)
    
    # 円を反転
    for circle in geometry.circles:
        mirrored_center = mirror_point(circle.center, axis, reference)
        mirrored.add_circle(mirrored_center, circle.radius)
    
    # ポリゴンを反転
    for polygon in geometry.polygons:
        mirrored_points = [mirror_point(p, axis, reference) for p in polygon.points]
        mirrored.add_polygon(mirrored_points)
    
    return mirrored


def mirror_drill_data(drill_data: DrillData, axis: MirrorAxis, reference: float = None) -> DrillData:
    """
    ドリルデータを反転する
    
    Args:
        drill_data: 反転するドリルデータ
        axis: 反転軸（X軸またはY軸）
        reference: 反転の基準位置（Noneの場合はバウンディングボックスの中心）
    
    Returns:
        反転後のドリルデータ
    """
    # 基準位置が指定されていない場合、バウンディングボックスの中心を使用
    if reference is None:
        min_point, max_point = drill_data.get_bounds()
        if axis == MirrorAxis.X:
            reference = (min_point.y + max_point.y) / 2
        else:
            reference = (min_point.x + max_point.x) / 2
    
    mirrored = DrillData()
    
    # すべての穴を反転
    for hole in drill_data.holes:
        mirrored_position = mirror_point(hole.position, axis, reference)
        mirrored.add_hole(mirrored_position, hole.diameter)
    
    return mirrored
