"""
ガーバーファイルパーサー

RS-274X形式のガーバーファイルを解析し、幾何データに変換する
"""

from typing import Optional, List, Tuple, Dict
import re
from core.geometry import Geometry, Point


class GerberParser:
    """ガーバーファイルパーサー"""
    
    def __init__(self):
        self.geometry = Geometry()
        self.current_x = 0.0
        self.current_y = 0.0
        self.format_spec = (4, 6)  # デフォルト: 整数4桁、小数6桁
        self.unit = "mm"  # mm または inch
        self.current_aperture = None
        self.apertures: Dict[str, dict] = {}  # アパーチャ定義
        self.draw_mode = False  # True: 描画モード, False: 移動モード
        
    def parse_file(self, filepath: str) -> Geometry:
        """
        ガーバーファイルを解析
        
        Args:
            filepath: ガーバーファイルのパス
        
        Returns:
            解析された幾何データ
        """
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
        
        return self.parse(content)
    
    def parse(self, content: str) -> Geometry:
        """
        ガーバーデータを解析
        
        Args:
            content: ガーバーファイルの内容
        
        Returns:
            解析された幾何データ
        """
        # 行ごとに処理（*で分割することもある）
        lines = content.split('\n')
        
        for line in lines:
            line = line.strip()
            if not line:
                continue
            
            # コメント
            if line.startswith('G04'):
                continue
            
            # 拡張コマンド（%で囲まれている）
            if line.startswith('%'):
                self._parse_extended_command(line)
                continue
            
            # アパーチャ定義（%ADD...）は拡張コマンド内で処理
            
            # 標準コマンド
            # G01: 線形補間（デフォルト）
            if line.startswith('G01'):
                continue  # 線形補間はデフォルトなので無視
            
            # D01: 描画しながら移動
            # D02: 描画せずに移動
            # D03: フラッシュ（スポット露光）
            
            # アパーチャ選択（D10, D11など）
            if re.match(r'^D\d+\*?$', line):
                aperture_code = line.replace('*', '')
                self.current_aperture = aperture_code
                continue
            
            # 座標データを解析（X, Y, I, J, D01/D02/D03を含む）
            if 'X' in line or 'Y' in line or 'D0' in line:
                self._parse_coordinate(line)
        
        return self.geometry
    
    def _parse_extended_command(self, line: str):
        """拡張コマンドを解析"""
        # フォーマット指定: %FSLAX46Y46*%
        if 'FS' in line:
            self._parse_format_spec(line)
        
        # 単位: %MOMM*% または %MOIN*%
        elif 'MO' in line:
            self._parse_unit(line)
        
        # アパーチャ定義: %ADD10C,0.100000*%
        elif 'ADD' in line:
            self._parse_aperture_definition(line)
    
    def _parse_format_spec(self, line: str):
        """フォーマット仕様を解析"""
        # %FSLAX46Y46*%のような形式
        match = re.search(r'X(\d)(\d)Y(\d)(\d)', line)
        if match:
            x_int, x_dec, y_int, y_dec = match.groups()
            self.format_spec = (int(x_int), int(x_dec))
    
    def _parse_unit(self, line: str):
        """単位を解析"""
        if 'MM' in line:
            self.unit = "mm"
        elif 'IN' in line:
            self.unit = "inch"
    
    def _parse_aperture_definition(self, line: str):
        """
        アパーチャ定義を解析
        
        例: %ADD10C,0.100000*%
        """
        # ADD<番号><タイプ>,<パラメータ>
        match = re.match(r'%ADD(\d+)([A-Z]),([^*]+)\*%', line)
        if match:
            aperture_num = f"D{match.group(1)}"
            aperture_type = match.group(2)
            params = match.group(3)
            
            self.apertures[aperture_num] = {
                'type': aperture_type,
                'params': params
            }
    
    def _parse_coordinate(self, line: str):
        """
        座標データを解析
        
        例: 
        - X0Y78232000D02*  (移動)
        - X73152000Y78232000D01*  (描画)
        """
        # 前の位置を保存
        prev_x = self.current_x
        prev_y = self.current_y
        
        # X座標を抽出
        x_match = re.search(r'X([+-]?\d+)', line)
        if x_match:
            self.current_x = self._convert_coordinate(x_match.group(1))
        
        # Y座標を抽出
        y_match = re.search(r'Y([+-]?\d+)', line)
        if y_match:
            self.current_y = self._convert_coordinate(y_match.group(1))
        
        # 操作コードを確認
        if 'D01' in line:
            # 描画しながら移動
            if self.draw_mode:
                # 前の位置から現在の位置まで線を引く
                start_point = Point(prev_x, prev_y)
                end_point = Point(self.current_x, self.current_y)
                self.geometry.add_line(start_point, end_point)
            self.draw_mode = True
            
        elif 'D02' in line:
            # 描画せずに移動（ペンアップ）
            self.draw_mode = False
            
        elif 'D03' in line:
            # フラッシュ（スポット露光）
            # 現在の位置にアパーチャの形状を配置
            # 簡易実装：円として追加
            if self.current_aperture and self.current_aperture in self.apertures:
                aperture = self.apertures[self.current_aperture]
                if aperture['type'] == 'C':  # 円形アパーチャ
                    try:
                        diameter = float(aperture['params'])
                        radius = diameter / 2
                        center = Point(self.current_x, self.current_y)
                        self.geometry.add_circle(center, radius)
                    except (ValueError, KeyError):
                        pass
    
    def _convert_coordinate(self, value_str: str) -> float:
        """
        座標文字列を浮動小数点数に変換
        
        Args:
            value_str: 座標文字列（例: "78232000"）
        
        Returns:
            変換された座標値（mm）
        """
        int_digits, dec_digits = self.format_spec
        total_digits = int_digits + dec_digits
        
        # 符号を処理
        if value_str.startswith('+') or value_str.startswith('-'):
            sign = 1 if value_str[0] == '+' else -1
            value_str = value_str[1:]
        else:
            sign = 1
        
        # 先頭のゼロを埋める（Leading zero omitted形式）
        value_str = value_str.zfill(total_digits)
        
        # 整数部と小数部に分割
        int_part = value_str[:int_digits]
        dec_part = value_str[int_digits:int_digits + dec_digits]
        
        value = sign * float(f"{int_part}.{dec_part}")
        
        # インチの場合はmmに変換
        if self.unit == "inch":
            value *= 25.4
        
        return value
