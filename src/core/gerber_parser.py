"""
ガーバ�Eファイルパ�Eサー

RS-274X形式�Eガーバ�Eファイルを解析し、幾何データに変換する、E
pcb-tools / FlatCAM の実裁E��参老E��した正確なパ�Eサー、E

重要な仕槁E
  - 座標�Eモーダル�E�最後�E値を保持�E�E
  - 操作コーチED01/D02/D03)もモーダル
  - G36/G37 はベタ塗りリージョン�E�カチE��パスには使わなぁE��E
  - アパ�Eチャ幁E= 銁E���E太ぁEↁELine.width として保孁E
"""

import re
from typing import Dict, List
from core.geometry import Geometry, Point, Polygon


# ---------------------------------------------------------------------------
# 正規表現パターン
# ---------------------------------------------------------------------------
_FS_RE   = re.compile(
    r'FS(?P<zero>[LTD])?(?P<notation>[AI])[NG0-9]*X(?P<xi>[0-7])(?P<xd>[0-7])Y(?P<yi>[0-7])(?P<yd>[0-7])')
_MO_RE   = re.compile(r'MO(?P<mo>MM|IN)')
_ADD_RE  = re.compile(r'ADD(?P<d>\d+)(?P<shape>[A-Z][A-Z0-9_.$]*),?(?P<mods>[^*%]*)')
_COORD_RE = re.compile(
    r'(?:G0?(?P<gcode>[123]))?' +
    r'(?:X(?P<x>[+-]?\d+))?' +
    r'(?:Y(?P<y>[+-]?\d+))?' +
    r'(?:I(?P<i>[+-]?\d+))?' +
    r'(?:J(?P<j>[+-]?\d+))?' +
    r'(?:D0?(?P<op>[123]))?')
_APER_RE  = re.compile(r'(?:G54)?D(?P<d>\d+)$')


class GerberParser:
    """RS-274X ガーバ�Eファイルパ�Eサー"""

    def __init__(self):
        self.geometry = Geometry()
        # --- フォーマット設宁E---
        self.fmt_int  = 4    # 整数桁数
        self.fmt_dec  = 6    # 小数桁数
        self.notation = 'absolute'
        self.unit     = 'mm'
        # --- アパ�Eチャ ---
        self.apertures: Dict[int, dict] = {}
        self.current_aperture = 0
        # --- モーダル状慁E---
        self.x   = 0.0
        self.y   = 0.0
        self.op  = 2          # 1=D01 2=D02 3=D03  (チE��ォルト�ED02)
        self.gcode = 1         # 1=G01(直線補間) 2=G02(時計回り円弧) 3=G03(反時計回り円弧)
        self.region_mode = False
        self.region_points: List[Point] = []

    # -----------------------------------------------------------------------
    # 公閁EAPI
    # -----------------------------------------------------------------------
    def parse_file(self, filepath: str) -> Geometry:
        with open(filepath, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
        return self.parse(content)

    def parse(self, content: str) -> Geometry:
        for cmd in self._split_commands(content):
            cmd = cmd.strip()
            if not cmd:
                continue
            if cmd.startswith('%') and cmd.endswith('%'):
                self._handle_param(cmd[1:-1])
            else:
                self._handle_command(cmd)
        return self.geometry

    # -----------------------------------------------------------------------
    # コマンド�E割
    # -----------------------------------------------------------------------
    def _split_commands(self, data: str):
        """% ブロチE��は %...% 全体を1コマンド、E��常コマンド�E * で終端"""
        i = 0
        n = len(data)
        buf: List[str] = []
        in_param = False

        while i < n:
            c = data[i]
            if c == '%':
                if in_param:
                    buf.append(c)
                    yield ''.join(buf)
                    buf = []
                    in_param = False
                else:
                    in_param = True
                    buf = ['%']
            elif c == '*' and not in_param:
                buf.append(c)
                yield ''.join(buf)
                buf = []
            elif c in ('\r', '\n') and not in_param:
                if buf:
                    yield ''.join(buf)
                    buf = []
            else:
                buf.append(c)
            i += 1

        if buf:
            yield ''.join(buf)

    # -----------------------------------------------------------------------
    # 拡張パラメータ処琁E
    # -----------------------------------------------------------------------
    def _handle_param(self, block: str):
        for stmt in block.split('*'):
            stmt = stmt.strip()
            if not stmt:
                continue
            if stmt.startswith('FS'):
                self._parse_fs(stmt)
            elif stmt.startswith('MO'):
                self._parse_mo(stmt)
            elif stmt.startswith('ADD'):
                self._parse_add(stmt)

    def _parse_fs(self, stmt: str):
        m = _FS_RE.search(stmt)
        if m:
            self.fmt_int  = int(m.group('xi'))
            self.fmt_dec  = int(m.group('xd'))
            self.notation = 'incremental' if m.group('notation') == 'I' else 'absolute'

    def _parse_mo(self, stmt: str):
        m = _MO_RE.search(stmt)
        if m:
            self.unit = 'mm' if m.group('mo') == 'MM' else 'inch'

    def _parse_add(self, stmt: str):
        """Aperture definition: extract and save width"""
        m = _ADD_RE.search(stmt)
        if not m:
            return
        d      = int(m.group('d'))
        shape  = m.group('shape')
        mods_s = m.group('mods').strip()

        width = 0.0
        try:
            params = [float(x) for x in mods_s.split('X') if x.strip()]
            if shape == 'C':
                width = params[0] if params else 0.0
            elif shape in ('R', 'O'):
                width = max(params[0], params[1]) if len(params) >= 2 else (params[0] if params else 0.0)
            elif shape == 'P':
                width = params[0] if params else 0.0
            else:
                width = params[0] if params else 0.0
        except (ValueError, IndexError):
            width = 0.0

        self.apertures[d] = {'type': shape, 'width': width}

    # -----------------------------------------------------------------------
    # 通常コマンド�E琁E
    # -----------------------------------------------------------------------
    def _handle_command(self, cmd: str):
        cmd = cmd.rstrip('*').strip()
        if not cmd:
            return

        # コメンチE
        if cmd.startswith('G04') or cmd.startswith('G4'):
            return

        # G36: リージョン開姁E
        if 'G36' in cmd:
            self.region_mode = True
            self.region_points = []
            return

        # G37: リージョン終亁E
        if 'G37' in cmd:
            if len(self.region_points) >= 3:
                self.geometry.polygons.append(Polygon(list(self.region_points)))
            self.region_mode = False
            self.region_points = []
            return

        # 非推奨の単位コーチE
        if cmd == 'G70':
            self.unit = 'inch'; return
        if cmd == 'G71':
            self.unit = 'mm';   return

        # プログラム終端
        if cmd in ('M02', 'M2', 'M00', 'M0'):
            return

        # アパ�Eチャ選抁E(D10以丁Eかつ D01/D02/D03 でなぁE
        m = _APER_RE.match(cmd)
        if m and int(m.group('d')) >= 10:
            self.current_aperture = int(m.group('d'))
            return

        # 座標�E操作コマンチE
        self._handle_coord(cmd)

    def _handle_coord(self, cmd: str):
        m = _COORD_RE.match(cmd)
        if not m:
            return

        prev_x, prev_y = self.x, self.y

        # Gコード更新（モーダル）
        if m.group('gcode') is not None:
            self.gcode = int(m.group('gcode'))

        # 座標更新（モーダル）
        if m.group('x') is not None:
            val = self._parse_coord(m.group('x'))
            self.x = (self.x + val) if self.notation == 'incremental' else val
        if m.group('y') is not None:
            val = self._parse_coord(m.group('y'))
            self.y = (self.y + val) if self.notation == 'incremental' else val

        # 円弧のI,J座標（中心相対座標）
        i_offset = 0.0
        j_offset = 0.0
        if m.group('i') is not None:
            i_offset = self._parse_coord(m.group('i'))
        if m.group('j') is not None:
            j_offset = self._parse_coord(m.group('j'))

        # 操作コード更新（モーダル）
        if m.group('op') is not None:
            self.op = int(m.group('op'))

        # 座標も操作コードも何もなければ スキップ
        if m.group('x') is None and m.group('y') is None and m.group('op') is None:
            return

        if self.op == 1:
            # D01: 切削移動
            if self.gcode == 1:
                # G01: 直線補間
                self._do_draw_line(prev_x, prev_y)
            elif self.gcode == 2:
                # G02: 時計回り円弧補間
                center_x = prev_x + i_offset
                center_y = prev_y + j_offset
                self._do_draw_arc(prev_x, prev_y, center_x, center_y, True)
            elif self.gcode == 3:
                # G03: 反時計回り円弧補間
                center_x = prev_x + i_offset
                center_y = prev_y + j_offset
                self._do_draw_arc(prev_x, prev_y, center_x, center_y, False)
        elif self.op == 2:
            # D02: 非切削移動
            if self.region_mode:
                if len(self.region_points) >= 3:
                    self.geometry.polygons.append(Polygon(list(self.region_points)))
                self.region_points = []
        elif self.op == 3:
            # D03: フラッシュ（パッド出力）
            self._do_flash()

    def _do_draw_line(self, px: float, py: float):
        """直線を描画"""
        if self.region_mode:
            if not self.region_points:
                self.region_points.append(Point(px, py))
            self.region_points.append(Point(self.x, self.y))
        else:
            width = self._current_width()
            self.geometry.add_line(Point(px, py), Point(self.x, self.y), width)

    def _do_draw_arc(self, start_x: float, start_y: float, center_x: float, center_y: float, clockwise: bool):
        """円弧を描画"""
        if self.region_mode:
            # リージョンモード内の円弧は簡易近似
            if not self.region_points:
                self.region_points.append(Point(start_x, start_y))
            self.region_points.append(Point(self.x, self.y))
        else:
            # 通常の円弧 - 幾何データに追加
            width = self._current_width()
            self.geometry.add_arc(
                Point(start_x, start_y),
                Point(self.x, self.y),
                Point(center_x, center_y),
                clockwise,
                width
            )

    def _do_flash(self):
        width = self._current_width()
        radius = width / 2.0
        if radius > 0:
            self.geometry.add_circle(Point(self.x, self.y), radius)

    def _current_width(self) -> float:
        ap = self.apertures.get(self.current_aperture)
        return ap['width'] if ap else 0.0

    # -----------------------------------------------------------------------
    # 座標変換
    # -----------------------------------------------------------------------
    def _parse_coord(self, s: str) -> float:
        if s is None:
            return 0.0
        sign = 1
        if s.startswith('+'):
            s = s[1:]
        elif s.startswith('-'):
            sign = -1
            s = s[1:]

        total = self.fmt_int + self.fmt_dec
        s = s.zfill(total)
        int_part = s[:self.fmt_int]
        dec_part = s[self.fmt_int:self.fmt_int + self.fmt_dec]
        value = sign * float(f"{int_part}.{dec_part}")
        if self.unit == 'inch':
            value *= 25.4
        return value

