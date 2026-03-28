"""
Gコード生成機能

ツールパスからCNC加工用のGコードを生成する
"""

from typing import List, Tuple
from core.geometry import Point


class GCodeGenerator:
    """Gコード生成器"""
    
    def __init__(self):
        self.feed_rate = 100.0  # 送り速度 (mm/min)
        self.rapid_feed_rate = 500.0  # 早送り速度 (mm/min)
        self.plunge_feed_rate = 50.0  # プランジ速度 (mm/min)
        self.travel_z = 2.0  # 移動時の高さ (mm)
        self.safe_z = 5.0  # 安全高さ（ホームポジション）(mm)
        self.cut_z = -0.1  # 切削深さ (mm)
        self.spindle_speed = 10000  # スピンドル回転数 (rpm)
        
    def generate_header(self) -> List[str]:
        """
        Gコードのヘッダーを生成
        
        Returns:
            Gコードの行のリスト
        """
        return [
            "(AltairCAM Generated G-Code)",
            "G21 (Metric units)",
            "G90 (Absolute positioning)",
            f"M3 S{self.spindle_speed} (Start spindle)",
            "G0 Z{:.3f} (Move to safe height)".format(self.safe_z),
            ""
        ]
    
    def generate_footer(self) -> List[str]:
        """
        Gコードのフッターを生成
        
        Returns:
            Gコードの行のリスト
        """
        return [
            "",
            "G0 Z{:.3f} (Move to safe height)".format(self.safe_z),
            "M5 (Stop spindle)",
            "M2 (End program)",
            ""
        ]
    
    def generate_move(self, x: float, y: float, z: float = None, feed: bool = False, plunge: bool = False) -> str:
        """
        移動コマンドを生成
        
        Args:
            x: X座標
            y: Y座標
            z: Z座標（Noneの場合は省略）
            feed: 送り速度を使用するか（True: G1, False: G0）
            plunge: プランジ（Z軸下降）かどうか
        
        Returns:
            Gコード行
        """
        command = "G1" if feed else "G0"
        z_part = f" Z{z:.3f}" if z is not None else ""
        
        # 送り速度の選択
        if feed:
            if plunge:
                # Z軸下降時はプランジ速度を使用
                feed_part = f" F{self.plunge_feed_rate:.1f}"
            else:
                # 通常の切削は送り速度を使用
                feed_part = f" F{self.feed_rate:.1f}"
        else:
            # 早送り（G0）は明示的に速度指定（一部のCNCで必要）
            feed_part = f" F{self.rapid_feed_rate:.1f}"
        
        return f"{command} X{x:.3f} Y{y:.3f}{z_part}{feed_part}"
    
    def generate_drill_path(self, holes: List[Tuple[Point, float]]) -> List[str]:
        """
        ドリル穴のツールパスをGコードに変換
        
        Args:
            holes: (位置, 直径)のタプルのリスト
        
        Returns:
            Gコードの行のリスト
        """
        gcode = []
        gcode.append("(Drill holes)")
        
        for position, diameter in holes:
            # 穴の位置まで移動（Travel Z）
            gcode.append(self.generate_move(position.x, position.y, self.travel_z))
            # 穴を開ける（プランジ速度でZ軸下降）
            gcode.append(self.generate_move(position.x, position.y, self.cut_z, feed=True, plunge=True))
            # 引き上げ（Travel Z）
            gcode.append(self.generate_move(position.x, position.y, self.travel_z))
        
        return gcode
    
    def generate_line_path(self, points: List[Point]) -> List[str]:
        """
        線分のツールパスをGコードに変換
        
        Args:
            points: 点のリスト（連続した線分）
        
        Returns:
            Gコードの行のリスト
        """
        if not points:
            return []
        
        gcode = []
        
        # 最初の点まで移動（Travel Z）
        gcode.append(self.generate_move(points[0].x, points[0].y, self.travel_z))
        # 切削深さまで降下（プランジ速度）
        gcode.append(self.generate_move(points[0].x, points[0].y, self.cut_z, feed=True, plunge=True))
        
        # 各点を切削しながら移動
        for point in points[1:]:
            gcode.append(self.generate_move(point.x, point.y, self.cut_z, feed=True))
        
        # 引き上げ（Travel Z）
        gcode.append(self.generate_move(points[-1].x, points[-1].y, self.travel_z))
        
        return gcode
    
    def generate_arc(self, start: Point, end: Point, center: Point, clockwise: bool) -> str:
        """
        円弧のGコードを生成
        
        Args:
            start: 開始点
            end: 終了点
            center: 中心点
            clockwise: 時計回りか
        
        Returns:
            Gコード行
        """
        # 中心からの相対座標を計算
        i = center.x - start.x
        j = center.y - start.y
        
        command = "G2" if clockwise else "G3"
        return f"{command} X{end.x:.3f} Y{end.y:.3f} I{i:.3f} J{j:.3f} F{self.feed_rate:.1f}"
    
    def generate_continuous_paths(self, paths: List[List[Point]], connect_tolerance: float = 0.001) -> List[str]:
        """
        複数のパスを切削。
        
        隣接するパスの端点が connect_tolerance (mm) 以内であれば
        Z を上げずに連続切削する。離れている場合は安全高さまで引き上げて
        移動してから再プランジする。
        
        Args:
            paths: パスのリスト（各パスは点のリスト）
            connect_tolerance: パスを連続とみなす距離の閾値 (mm)
        
        Returns:
            Gコードの行のリスト
        """
        if not paths:
            return []
        
        gcode = []
        gcode.append("(Continuous cutting paths)")
        
        prev_end: Point = None
        in_cut = False

        for path in paths:
            if not path:
                continue
            
            start = path[0]
            
            if prev_end is not None:
                # 前のパス末点と現在のパス始点の距離を計算
                dist = ((start.x - prev_end.x) ** 2 + (start.y - prev_end.y) ** 2) ** 0.5
                if dist <= connect_tolerance:
                    # 隣接しているのでそのまま続行（Z 上げない）
                    pass
                else:
                    # 離れているので一度 Z を上げて移動してから再プランジ
                    gcode.append(self.generate_move(prev_end.x, prev_end.y, self.travel_z))
                    gcode.append(self.generate_move(start.x, start.y, self.travel_z))
                    gcode.append(self.generate_move(start.x, start.y, self.cut_z, feed=True, plunge=True))
                    in_cut = True
            
            if prev_end is None:
                # 最初のパス
                gcode.append(self.generate_move(start.x, start.y, self.travel_z))
                gcode.append(self.generate_move(start.x, start.y, self.cut_z, feed=True, plunge=True))
                in_cut = True
            
            # パスを切削
            for point in path[1:]:
                gcode.append(self.generate_move(point.x, point.y, self.cut_z, feed=True))
            
            prev_end = path[-1]
        
        # 最後に引き上げ
        if prev_end is not None and in_cut:
            gcode.append(self.generate_move(prev_end.x, prev_end.y, self.travel_z))
        
        return gcode
    
    def save_to_file(self, gcode_lines: List[str], filepath: str):
        """
        Gコードをファイルに保存
        
        Args:
            gcode_lines: Gコードの行のリスト
            filepath: 保存先のファイルパス
        """
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write('\n'.join(gcode_lines))
