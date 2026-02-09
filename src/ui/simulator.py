"""
Gコード解析器とシミュレーター

Gコードを解析してツールパスを抽出し、切削シミュレーションを実行
"""

import re
from typing import List, Tuple, Optional
from core.geometry import Point


class GCodeCommand:
    """Gコードコマンド"""
    def __init__(self, line: str):
        self.line = line.strip()
        self.x: Optional[float] = None
        self.y: Optional[float] = None
        self.z: Optional[float] = None
        self.feed_rate: Optional[float] = None
        self.is_rapid = False
        self.is_linear = False
        self.is_drill = False
        
        self._parse()
    
    def _parse(self):
        """コマンドを解析"""
        # G00: 早送り移動
        if 'G00' in self.line or 'G0' in self.line:
            self.is_rapid = True
        
        # G01: 直線補間（切削）
        if 'G01' in self.line or 'G1' in self.line:
            self.is_linear = True
        
        # Z軸がマイナスならドリル動作
        z_match = re.search(r'Z([+-]?[\d.]+)', self.line)
        if z_match:
            self.z = float(z_match.group(1))
            if self.z < 0:
                self.is_drill = True
        
        # X座標
        x_match = re.search(r'X([+-]?[\d.]+)', self.line)
        if x_match:
            self.x = float(x_match.group(1))
        
        # Y座標
        y_match = re.search(r'Y([+-]?[\d.]+)', self.line)
        if y_match:
            self.y = float(y_match.group(1))
        
        # 送り速度
        f_match = re.search(r'F([+-]?[\d.]+)', self.line)
        if f_match:
            self.feed_rate = float(f_match.group(1))
    
    def __repr__(self):
        return f"GCodeCommand(x={self.x}, y={self.y}, z={self.z}, rapid={self.is_rapid}, linear={self.is_linear})"


class GCodeSimulator:
    """Gコードシミュレーター"""
    
    def __init__(self):
        self.current_x = 0.0
        self.current_y = 0.0
        self.current_z = 0.0
        self.safe_z = 2.0  # 安全高さ
        
    def parse_gcode(self, gcode_lines: List[str]) -> List[Tuple[Point, Point, str]]:
        """
        Gコードを解析してツールパスを抽出
        
        Args:
            gcode_lines: Gコードの行リスト
        
        Returns:
            [(始点, 終点, タイプ), ...] のリスト
            タイプ: 'rapid'（早送り）, 'cut'（切削）, 'drill'（ドリル）
        """
        toolpaths = []
        
        for line in gcode_lines:
            # コメントと空行をスキップ
            if not line or line.startswith(';') or line.startswith('('):
                continue
            
            cmd = GCodeCommand(line)
            
            # 座標が指定されている場合のみ処理
            if cmd.x is not None or cmd.y is not None or cmd.z is not None:
                # 新しい座標（指定されていない場合は現在位置を維持）
                new_x = cmd.x if cmd.x is not None else self.current_x
                new_y = cmd.y if cmd.y is not None else self.current_y
                new_z = cmd.z if cmd.z is not None else self.current_z
                
                # 始点と終点
                start = Point(self.current_x, self.current_y, self.current_z)
                end = Point(new_x, new_y, new_z)
                
                # タイプを判定
                if cmd.is_drill or (cmd.z is not None and cmd.z < -0.01):
                    path_type = 'drill'
                elif cmd.is_rapid:
                    path_type = 'rapid'
                elif cmd.is_linear:
                    path_type = 'cut'
                else:
                    path_type = 'rapid'
                
                # 移動がある場合のみ追加
                if start.x != end.x or start.y != end.y or start.z != end.z:
                    toolpaths.append((start, end, path_type))
                
                # 現在位置を更新
                self.current_x = new_x
                self.current_y = new_y
                self.current_z = new_z
        
        return toolpaths
    
    def load_gcode_file(self, filepath: str) -> List[Tuple[Point, Point, str]]:
        """
        Gコードファイルを読み込んでツールパスを抽出
        
        Args:
            filepath: Gコードファイルのパス
        
        Returns:
            ツールパスのリスト
        """
        with open(filepath, 'r', encoding='utf-8') as f:
            lines = f.readlines()
        
        return self.parse_gcode(lines)
