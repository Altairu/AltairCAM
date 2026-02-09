"""
Excellonドリルファイルパーサー

Excellon形式のドリルファイルを解析し、ドリルデータに変換する
"""

from typing import Dict, Tuple
import re
from core.geometry import DrillData, Point


class ExcellonParser:
    """Excellonドリルファイルパーサー"""
    
    def __init__(self):
        self.drill_data = DrillData()
        self.current_x = 0.0
        self.current_y = 0.0
        self.current_tool = None
        self.tools: Dict[str, float] = {}  # ツール番号 -> 直径
        self.unit = "mm"  # mm または inch
        self.format_spec = (3, 3)  # デフォルト: 整数3桁、小数3桁
        
    def parse_file(self, filepath: str) -> DrillData:
        """
        ドリルファイルを解析
        
        Args:
            filepath: ドリルファイルのパス
        
        Returns:
            解析されたドリルデータ
        """
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
        
        return self.parse(content)
    
    def parse(self, content: str) -> DrillData:
        """
        ドリルデータを解析
        
        Args:
            content: ドリルファイルの内容
        
        Returns:
            解析されたドリルデータ
        """
        lines = content.split('\n')
        
        for line in lines:
            line = line.strip()
            if not line or line.startswith(';'):
                continue
            
            # ヘッダー部分の解析
            if line == 'M48':  # ヘッダー開始
                continue
            elif line == '%':  # ヘッダー終了
                continue
            
            # 単位を解析
            elif line == 'METRIC':
                self.unit = "mm"
            elif line == 'INCH':
                self.unit = "inch"
            
            # フォーマット指定を解析
            elif line.startswith('FMAT'):
                # FMAT,2 のような形式（通常は無視）
                pass
            
            # ツール定義を解析
            elif line.startswith('T') and 'C' in line:
                self._parse_tool_definition(line)
            
            # ツール選択
            elif line.startswith('T') and 'C' not in line:
                self._parse_tool_selection(line)
            
            # 座標データを解析
            elif line.startswith('X') or line.startswith('Y'):
                self._parse_coordinate(line)
            
            # プログラム終了
            elif line == 'M30':
                break
        
        return self.drill_data
    
    def _parse_tool_definition(self, line: str):
        """
        ツール定義を解析
        
        例: T01C0.800
        """
        match = re.match(r'T(\d+)C([\d.]+)', line)
        if match:
            tool_number = match.group(1)
            diameter = float(match.group(2))
            
            # インチの場合はmmに変換
            if self.unit == "inch":
                diameter *= 25.4
            
            self.tools[tool_number] = diameter
    
    def _parse_tool_selection(self, line: str):
        """
        ツール選択を解析
        
        例: T01
        """
        match = re.match(r'T(\d+)', line)
        if match:
            self.current_tool = match.group(1)
    
    def _parse_coordinate(self, line: str):
        """
        座標データを解析し、穴を追加
        
        例: 
        - X012345Y067890 (整数形式)
        - X22.606Y65.532 (decimal形式)
        """
        # X座標を抽出
        x_match = re.search(r'X([+-]?[\d.]+)', line)
        if x_match:
            self.current_x = self._convert_coordinate(x_match.group(1))
        
        # Y座標を抽出
        y_match = re.search(r'Y([+-]?[\d.]+)', line)
        if y_match:
            self.current_y = self._convert_coordinate(y_match.group(1))
        
        # 穴を追加
        if self.current_tool and self.current_tool in self.tools:
            diameter = self.tools[self.current_tool]
            position = Point(self.current_x, self.current_y)
            self.drill_data.add_hole(position, diameter)
    
    def _convert_coordinate(self, value_str: str) -> float:
        """
        座標文字列を浮動小数点数に変換
        
        Args:
            value_str: 座標文字列（例: "22.606" または "012345"）
        
        Returns:
            変換された座標値（mm）
        """
        # 小数点が含まれている場合はdecimal形式
        if '.' in value_str:
            value = float(value_str)
        else:
            # 整数形式（Leading zero suppression）
            int_digits, dec_digits = self.format_spec
            total_digits = int_digits + dec_digits
            
            # 符号を処理
            if value_str.startswith('+') or value_str.startswith('-'):
                sign = 1 if value_str[0] == '+' else -1
                value_str = value_str[1:]
            else:
                sign = 1
            
            # 前ゼロ埋め
            value_str = value_str.zfill(total_digits)
            
            # 整数部と小数部に分割
            int_part = value_str[:int_digits]
            dec_part = value_str[int_digits:int_digits + dec_digits]
            
            value = sign * float(f"{int_part}.{dec_part}")
        
        # インチの場合はmmに変換
        if self.unit == "inch":
            value *= 25.4
        
        return value
