"""
2Dプレビュー表示機能

ガーバーデータとドリルデータを視覚化する
"""

import matplotlib.pyplot as plt
from matplotlib.backends.backend_tkagg import FigureCanvasTkAgg
from matplotlib.figure import Figure
from typing import Optional
from core.geometry import Geometry, DrillData


class PreviewCanvas:
    """2Dプレビューキャンバス"""
    
    def __init__(self, parent, width=8, height=6):
        """
        Args:
            parent: 親ウィジェット（Tkinter）
            width: 図のサイズ（インチ）
            height: 図のサイズ（インチ）
        """
        self.parent = parent
        
        # Matplotlibの図を作成
        self.figure = Figure(figsize=(width, height), dpi=100)
        self.ax = self.figure.add_subplot(111)
        
        # Tkinterキャンバスを作成
        self.canvas = FigureCanvasTkAgg(self.figure, parent)
        self.canvas_widget = self.canvas.get_tk_widget()
        
        # 初期設定
        self.ax.set_aspect('equal')
        self.ax.grid(True, alpha=0.3)
        self.ax.set_xlabel('X (mm)')
        self.ax.set_ylabel('Y (mm)')
        self.ax.set_title('PCB Preview')
        
        # データを保持
        self.geometry: Optional[Geometry] = None
        self.drill_data: Optional[DrillData] = None
        
    def pack(self, **kwargs):
        """キャンバスウィジェットをpack"""
        self.canvas_widget.pack(**kwargs)
    
    def clear(self):
        """プレビューをクリア"""
        self.ax.clear()
        self.ax.set_aspect('equal')
        self.ax.grid(True, alpha=0.3)
        self.ax.set_xlabel('X (mm)')
        self.ax.set_ylabel('Y (mm)')
        self.ax.set_title('PCB Preview')
        self.canvas.draw()
    
    def draw_geometry(self, geometry: Geometry, tool_diameter: float = 0.5, 
                     color='blue', label='Gerber', alpha=1.0):
        """
        幾何データを描画
        
        Args:
            geometry: 幾何データ
            tool_diameter: ツール直径（mm）- 線幅の表示に使用
            color: 線の色
            label: ラベル
            alpha: 透明度
        """
        self.geometry = geometry

        # 各線分をアパーチャ幅（line.width）で描画
        # Matplotlib の linewidth はポイント単位（1pt ≒ 1/72 inch）
        # 実寸 1mm ≒ 2.83 pt @ 72dpi だが、Figure の DPI と axes の
        # 実スケールによって変わるため、ここでは見やすさ優先で簡易計算する。
        PTS_PER_MM = 2.83

        for i, line in enumerate(geometry.lines):
            # line.width: アパーチャ幅(mm)。0 なら最小幅で描画
            lw = max(line.width * PTS_PER_MM, 0.5)
            self.ax.plot(
                [line.start.x, line.end.x],
                [line.start.y, line.end.y],
                color=color,
                linewidth=lw,
                solid_capstyle='round',   # 線端を丸くして銅箔らしく見せる
                alpha=alpha,
                label=label if i == 0 else ""
            )
        
        # 円（パッド）を描画 - 塗りつぶしで実寸表示
        for circle in geometry.circles:
            circle_patch = plt.Circle(
                (circle.center.x, circle.center.y),
                circle.radius,
                color=color,
                fill=True,
                alpha=alpha * 0.8,
            )
            self.ax.add_patch(circle_patch)
        
        # 円弧を描画
        for arc in geometry.arcs:
            import math
            
            # 半径を計算
            dx = arc.start.x - arc.center.x
            dy = arc.start.y - arc.center.y
            radius = math.sqrt(dx*dx + dy*dy)
            
            if radius > 0.001:
                # 開始角度と終了角度を計算
                start_angle = math.atan2(arc.start.y - arc.center.y, arc.start.x - arc.center.x)
                end_angle = math.atan2(arc.end.y - arc.center.y, arc.end.x - arc.center.x)
                
                # 円弧の角度範囲を計算
                if arc.clockwise:
                    # 時計回り
                    if end_angle > start_angle:
                        end_angle -= 2 * math.pi
                    angles = [start_angle - t * (start_angle - end_angle) for t in [i/50 for i in range(51)]]
                else:
                    # 反時計回り
                    if end_angle < start_angle:
                        end_angle += 2 * math.pi
                    angles = [start_angle + t * (end_angle - start_angle) for t in [i/50 for i in range(51)]]
                
                # 円弧の座標を計算
                arc_x = [arc.center.x + radius * math.cos(angle) for angle in angles]
                arc_y = [arc.center.y + radius * math.sin(angle) for angle in angles]
                
                self.ax.plot(
                    arc_x,
                    arc_y,
                    color=color,
                    linewidth=linewidth,
                    alpha=alpha
                )
            else:
                # 退化した円弧 - 直線として表示
                self.ax.plot(
                    [arc.start.x, arc.end.x],
                    [arc.start.y, arc.end.y],
                    color=color,
                    linewidth=linewidth,
                    alpha=alpha
                )
        
        # ポリゴン（ベタ塗り領域）は輪郭線のみ薄く表示（塗りつぶしなし）
        # GNDベタなど大きなポリゴンが全体を塗りつぶして視認性を損なうのを防ぐ
        for polygon in geometry.polygons:
            if len(polygon.points) < 2:
                continue
            xs = [p.x for p in polygon.points]
            ys = [p.y for p in polygon.points]
            if xs[0] != xs[-1] or ys[0] != ys[-1]:
                xs.append(xs[0])
                ys.append(ys[0])
            self.ax.plot(xs, ys, color=color, linewidth=0.5, alpha=0.3, linestyle='dotted')
    
    def draw_drill_data(self, drill_data: DrillData, tool_diameter: float = None,
                       color='red', label='Drill', alpha=0.8):
        """
        ドリルデータを描画
        
        Args:
            drill_data: ドリルデータ
            tool_diameter: ツール直径（mm）- 使用しない（穴の直径を使用）
            color: マーカーの色
            label: ラベル
            alpha: 透明度
        """
        self.drill_data = drill_data
        
        if not drill_data.holes:
            return
        
        # 各穴を直径に応じて描画
        for i, hole in enumerate(drill_data.holes):
            # 穴の円を描画（塗りつぶし）
            circle_patch = plt.Circle(
                (hole.position.x, hole.position.y),
                hole.diameter / 2,
                color=color,
                fill=True,
                alpha=alpha,
                label=label if i == 0 else ""
            )
            self.ax.add_patch(circle_patch)
            
            # 穴の外周線を描画
            circle_outline = plt.Circle(
                (hole.position.x, hole.position.y),
                hole.diameter / 2,
                color='darkred',
                fill=False,
                linewidth=0.5,
                alpha=1.0
            )
            self.ax.add_patch(circle_patch)
    
    def auto_scale(self):
        """自動スケール調整"""
        # データの範囲を取得
        if self.geometry:
            min_point, max_point = self.geometry.get_bounds()
            margin = 5  # mm
            self.ax.set_xlim(min_point.x - margin, max_point.x + margin)
            self.ax.set_ylim(min_point.y - margin, max_point.y + margin)
        elif self.drill_data:
            min_point, max_point = self.drill_data.get_bounds()
            margin = 5  # mm
            self.ax.set_xlim(min_point.x - margin, max_point.x + margin)
            self.ax.set_ylim(min_point.y - margin, max_point.y + margin)
    
    def show_legend(self):
        """凡例を表示"""
        self.ax.legend(loc='upper right')
    
    def refresh(self):
        """描画を更新"""
        self.canvas.draw()
    
    def update_preview(self, b_cu_geometry=None, b_cu_tool_diameter=0.5,
                      edge_cuts_geometry=None, edge_cuts_tool_diameter=0.5,
                      drill_data=None, drill_tool_diameter=None):
        """
        プレビューを更新（3つのレイヤー対応）
        
        Args:
            b_cu_geometry: B_Cu（銅箔）の幾何データ
            b_cu_tool_diameter: B_Cuのツール直径
            edge_cuts_geometry: Edge_Cuts（基板外形）の幾何データ
            edge_cuts_tool_diameter: Edge_Cutsのツール直径
            drill_data: ドリルデータ
            drill_tool_diameter: ドリルのツール直径（未使用、穴の直径を使用）
        """
        self.clear()
        
        # B_Cu（銅箔層）を描画
        if b_cu_geometry:
            self.draw_geometry(b_cu_geometry, 
                             tool_diameter=b_cu_tool_diameter,
                             color='blue', 
                             label='B_Cu',
                             alpha=0.7)
        
        # Edge_Cuts（基板外形）を描画
        if edge_cuts_geometry:
            self.draw_geometry(edge_cuts_geometry,
                             tool_diameter=edge_cuts_tool_diameter,
                             color='green',
                             label='Edge_Cuts',
                             alpha=0.9)
        
        # ドリル穴を描画
        if drill_data:
            self.draw_drill_data(drill_data, 
                               tool_diameter=drill_tool_diameter,
                               color='red', 
                               label='Drill',
                               alpha=0.6)
        
        if b_cu_geometry or edge_cuts_geometry or drill_data:
            self.auto_scale()
            self.show_legend()
        
        self.refresh()
