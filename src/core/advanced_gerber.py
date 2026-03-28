"""
改良版ガーバーファイルパーサー

FlatCAMで証明されたガーバー仕様拡張に対応した、より完全なパーサー
"""

import re
import math
from typing import Dict, List, Tuple, Optional
from core.geometry import Geometry, Point, Polygon


class AdvancedGerberParser:
    """高度なガーバーファイルパーサー"""
    
    def __init__(self):
        self.geometry = Geometry()
        
        # フォーマット設定
        self.fmt_int = 4
        self.fmt_dec = 6
        self.notation = 'absolute'  # absolute or incremental
        self.unit = 'mm'  # mm or inch
        
        # アパーチャ
        self.apertures: Dict[int, dict] = {}
        self.current_aperture = 0
        
        # モーダル状態
        self.x = 0.0
        self.y = 0.0
        self.i = 0.0
        self.j = 0.0
        self.op = 2  # D01, D02, D03
        self.gcode = 1  # G01, G02, G03
        
        # リージョン（ベタ塗り）モード
        self.region_mode = False
        self.region_points: List[Point] = []
        
    def parse_file(self, filepath: str) -> Geometry:
        """ファイルからガーバーを解析"""
        with open(filepath, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
        return self.parse(content)
    
    def parse(self, content: str) -> Geometry:
        """ガーバーコンテンツを解析"""
        # コマンド分割
        commands = self._split_commands(content)
        
        for cmd in commands:
            cmd = cmd.strip()
            if not cmd:
                continue
                
            # エクステンデッド属性ブロック
            if cmd.startswith('%') and cmd.endswith('%'):
                self._parse_extended_attr(cmd[1:-1])
            else:
                self._parse_command(cmd)
        
        # 最後のリージョンがあれば閉じる
        if self.region_mode and len(self.region_points) >= 3:
            self.geometry.polygons.append(Polygon(list(self.region_points)))
            self.region_mode = False
            self.region_points = []
        
        return self.geometry
    
    def _split_commands(self, content: str) -> List[str]:
        """コマンドを分割"""
        commands = []
        buffer = ""
        in_extended = False
        
        for char in content:
            if char == '%':
                if in_extended:
                    buffer += char
                    commands.append(buffer)
                    buffer = ""
                    in_extended = False
                else:
                    if buffer:
                        commands.append(buffer)
                    buffer = "%"
                    in_extended = True
            elif char == '*' and not in_extended:
                buffer += char
                commands.append(buffer)
                buffer = ""
            elif char not in ('\r', '\n') or in_extended:
                buffer += char
        
        if buffer:
            commands.append(buffer)
        
        return commands
    
    def _parse_extended_attr(self, block: str):
        """エクステンデッド属性を解析"""
        statements = block.split('*')
        
        for stmt in statements:
            stmt = stmt.strip()
            if not stmt:
                continue
            
            if stmt.startswith('FS'):
                self._parse_fs(stmt)
            elif stmt.startswith('MO'):
                self._parse_mo(stmt)
            elif stmt.startswith('ADD'):
                self._parse_add(stmt)
            elif stmt.startswith('AS'):
                self._parse_as(stmt)
    
    def _parse_fs(self, stmt: str):
        """フォーマット仕様"""
        m = re.search(r'FS([LTD])?([AI])([NG])?X(\d)(\d)Y(\d)(\d)', stmt)
        if m:
            self.fmt_int = int(m.group(4))
            self.fmt_dec = int(m.group(5))
            self.notation = 'incremental' if m.group(2) == 'I' else 'absolute'
    
    def _parse_mo(self, stmt: str):
        """単位"""
        if 'MM' in stmt:
            self.unit = 'mm'
        elif 'IN' in stmt:
            self.unit = 'inch'
    
    def _parse_add(self, stmt: str):
        """アパーチャ定義"""
        m = re.search(r'ADD(\d+)([CROP][^,]*),?(.+)', stmt, re.IGNORECASE)
        if not m:
            return
        
        d = int(m.group(1))
        shape = m.group(2)[0].upper()
        params_str = m.group(3) if m.group(3) else ""
        
        # パラメータ抽出
        params = []
        try:
            for p in params_str.split('X'):
                p = p.strip()
                if p:
                    params.append(float(p))
        except:
            pass
        
        # 形状に応じた幅計算
        width = 0.0
        if shape == 'C':  # Circle
            width = params[0] if params else 0.0
        elif shape == 'R':  # Rectangle
            width = max(params[0], params[1]) if len(params) >= 2 else params[0] if params else 0.0
        elif shape == 'O':  # Obround
            width = max(params[0], params[1]) if len(params) >= 2 else params[0] if params else 0.0
        elif shape == 'P':  # Polygon
            width = params[0] if params else 0.0
        else:
            width = params[0] if params else 0.0
        
        self.apertures[d] = {'shape': shape, 'width': width, 'params': params}
    
    def _parse_as(self, stmt: str):
        """アパーチャ選択（D10-D99）"""
        m = re.search(r'AS(\d+)', stmt)
        if m:
            d = int(m.group(1))
            if d >= 10:
                self.current_aperture = d
    
    def _parse_command(self, cmd: str):
        """通常コマンド"""
        cmd = cmd.rstrip('*').strip()
        if not cmd:
            return
        
        # コメント
        if cmd.startswith('G04') or cmd.startswith('G4'):
            return
        
        # リージョン開始
        if 'G36' in cmd:
            self.region_mode = True
            self.region_points = []
            return
        
        # リージョン終了
        if 'G37' in cmd:
            if self.region_mode and len(self.region_points) >= 3:
                self.geometry.polygons.append(Polygon(list(self.region_points)))
            self.region_mode = False
            self.region_points = []
            return
        
        # 単位（G70/G71は非推奨）
        if cmd == 'G70':
            self.unit = 'inch'
            return
        if cmd == 'G71':
            self.unit = 'mm'
            return
        
        # プログラム終了
        if cmd in ('M02', 'M2', 'M00', 'M0'):
            return
        
        # D コード（アパーチャ選択、10以上）
        m = re.match(r'D(\d+)$', cmd)
        if m:
            d = int(m.group(1))
            if d >= 10:
                self.current_aperture = d
            return
        
        # 座標＆操作コマンド
        self._parse_coordinate_command(cmd)
    
    def _parse_coordinate_command(self, cmd: str):
        """座標と操作コマンドを解析"""
        # パターン
        pattern = (
            r'(?:G0?([123]))?'  # G01,G02,G03
            r'(?:X([+-]?\d+))?'  # X
            r'(?:Y([+-]?\d+))?'  # Y
            r'(?:I([+-]?\d+))?'  # I
            r'(?:J([+-]?\d+))?'  # J
            r'(?:D0?([1-3]))?'   # D01,D02,D03
        )
        
        m = re.match(pattern, cmd)
        if not m:
            return
        
        prev_x = self.x
        prev_y = self.y
        
        # Gコード
        if m.group(1):
            self.gcode = int(m.group(1))
        
        # 座標
        if m.group(2):
            self.x = self._parse_coord_value(m.group(2))
        if m.group(3):
            self.y = self._parse_coord_value(m.group(3))
        
        # I, J（円弧用）
        if m.group(4):
            self.i = self._parse_coord_value(m.group(4))
        if m.group(5):
            self.j = self._parse_coord_value(m.group(5))
        
        # 操作コード
        if m.group(6):
            self.op = int(m.group(6))
        
        # 処理
        if self.op == 1:
            # D01: 切削移動
            if self.gcode == 1:
                # G01: 直線
                self._draw_line(prev_x, prev_y)
            elif self.gcode in (2, 3):
                # G02/G03: 円弧
                self._draw_arc(prev_x, prev_y, self.gcode == 2)
        elif self.op == 2:
            # D02: 非切削移動
            if self.region_mode:
                if len(self.region_points) >= 3:
                    self.geometry.polygons.append(Polygon(list(self.region_points)))
                self.region_points = []
        elif self.op == 3:
            # D03: フラッシュ
            self._do_flash()
    
    def _parse_coord_value(self, s: str) -> float:
        """座標値を解析"""
        if not s:
            return 0.0
        
        sign = 1.0
        if s.startswith('-'):
            sign = -1.0
            s = s[1:]
        elif s.startswith('+'):
            s = s[1:]
        
        total_digits = self.fmt_int + self.fmt_dec
        s = s.zfill(total_digits)
        
        int_part = s[:self.fmt_int]
        dec_part = s[self.fmt_int:self.fmt_int + self.fmt_dec]
        
        value = sign * float(f"{int_part}.{dec_part}")
        
        if self.unit == 'inch':
            value *= 25.4
        
        if self.notation == 'incremental':
            # インクリメンタル座標は相対から絶対に変換
            if s == int_part:  # X/Y座標の場合
                pass  # すでに相対値
        
        return value
    
    def _draw_line(self, prev_x: float, prev_y: float):
        """直線を描画"""
        start = Point(prev_x, prev_y)
        end = Point(self.x, self.y)
        width = self._get_aperture_width()
        
        if self.region_mode:
            if not self.region_points or self.region_points[-1] != start:
                if not self.region_points:
                    self.region_points.append(start)
            self.region_points.append(end)
        else:
            self.geometry.add_line(start, end, width)
    
    def _draw_arc(self, prev_x: float, prev_y: float, clockwise: bool):
        """円弧を描画"""
        start = Point(prev_x, prev_y)
        end = Point(self.x, self.y)
        center = Point(prev_x + self.i, prev_y + self.j)
        
        if self.region_mode:
            # リージョン内の円弧は簡易近似
            if not self.region_points or self.region_points[-1] != start:
                if not self.region_points:
                    self.region_points.append(start)
            self.region_points.append(end)
        else:
            width = self._get_aperture_width()
            self.geometry.add_arc(start, end, center, clockwise, width)
    
    def _do_flash(self):
        """フラッシュ（パッド）を出力"""
        width = self._get_aperture_width()
        radius = width / 2.0
        
        if radius > 0:
            self.geometry.add_circle(Point(self.x, self.y), radius)
    
    def _get_aperture_width(self) -> float:
        """現在のアパーチャ幅を取得"""
        ap = self.apertures.get(self.current_aperture)
        return ap['width'] if ap else 0.0

