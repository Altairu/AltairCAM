"""
3Dプレビューウィンドウ

Matplotlib 3Dを使用して基板データを3D表示
"""

import tkinter as tk
from tkinter import ttk
import matplotlib.pyplot as plt
from matplotlib.backends.backend_tkagg import FigureCanvasTkAgg, NavigationToolbar2Tk
from mpl_toolkits.mplot3d import Axes3D
from matplotlib.figure import Figure
import numpy as np

from core.geometry import Geometry, DrillData


class Preview3DWindow:
    """3Dプレビューウィンドウ"""
    
    def __init__(self, parent, b_cu_geometry=None, b_cu_tool_diameter=0.5,
                 edge_cuts_geometry=None, edge_cuts_tool_diameter=0.5,
                 drill_data=None, drill_tool_diameter=None):
        """
        3Dプレビューウィンドウを初期化
        
        Args:
            parent: 親ウィンドウ
            b_cu_geometry: B_Cuの幾何データ
            b_cu_tool_diameter: B_Cuのツール直径
            edge_cuts_geometry: Edge_Cutsの幾何データ
            edge_cuts_tool_diameter: Edge_Cutsのツール直径
            drill_data: ドリルデータ
            drill_tool_diameter: ドリルツール直径
        """
        self.window = tk.Toplevel(parent)
        self.window.title("3D Preview - AltairCAM")
        self.window.geometry("1000x800")
        
        # データを保存
        self.b_cu_geometry = b_cu_geometry
        self.b_cu_tool_diameter = b_cu_tool_diameter
        self.edge_cuts_geometry = edge_cuts_geometry
        self.edge_cuts_tool_diameter = edge_cuts_tool_diameter
        self.drill_data = drill_data
        self.drill_tool_diameter = drill_tool_diameter
        
        # 3Dプロット用のフィギュアを作成
        self.fig = Figure(figsize=(10, 8), facecolor='#2b2b2b')
        self.ax = self.fig.add_subplot(111, projection='3d', facecolor='#1e1e1e')
        
        # キャンバスを作成
        self.canvas = FigureCanvasTkAgg(self.fig, master=self.window)
        self.canvas.get_tk_widget().pack(fill=tk.BOTH, expand=True)
        
        # ツールバーを作成
        toolbar_frame = ttk.Frame(self.window)
        toolbar_frame.pack(fill=tk.X)
        
        toolbar = NavigationToolbar2Tk(self.canvas, toolbar_frame)
        toolbar.update()
        
        # コントロールパネル
        control_frame = ttk.Frame(self.window, padding="10")
        control_frame.pack(fill=tk.X)
        
        ttk.Button(control_frame, text="リセット視点", 
                  command=self.reset_view).pack(side=tk.LEFT, padx=5)
        ttk.Button(control_frame, text="上面図", 
                  command=self.top_view).pack(side=tk.LEFT, padx=5)
        ttk.Button(control_frame, text="側面図", 
                  command=self.side_view).pack(side=tk.LEFT, padx=5)
        ttk.Button(control_frame, text="閉じる", 
                  command=self.window.destroy).pack(side=tk.RIGHT, padx=5)
        
        # 3Dデータを描画
        self.render_3d()
    
    def render_3d(self):
        """3Dデータを描画"""
        self.ax.clear()
        
        # 軸ラベル設定
        self.ax.set_xlabel('X (mm)', color='white')
        self.ax.set_ylabel('Y (mm)', color='white')
        self.ax.set_zlabel('Z (mm)', color='white')
        self.ax.set_title('PCB 3D Preview', color='white', fontsize=14, pad=20)
        
        # グリッド設定
        self.ax.grid(True, alpha=0.3)
        self.ax.xaxis.pane.fill = False
        self.ax.yaxis.pane.fill = False
        self.ax.zaxis.pane.fill = False
        
        # 軸の色を設定
        self.ax.tick_params(colors='white')
        
        # 基板表面（Z=0）
        self._draw_board_surface()
        
        # B_Cu（銅箔層）を描画 - Z = 0（表面）
        if self.b_cu_geometry:
            self._draw_geometry_3d(self.b_cu_geometry, 
                                  z_level=0.0,
                                  color='#4488ff',
                                  label='B_Cu',
                                  linewidth=2)
        
        # Edge_Cuts（基板外形）を描画 - Z = 基板厚全体
        if self.edge_cuts_geometry:
            # 表面と裏面の両方に描画
            self._draw_geometry_3d(self.edge_cuts_geometry,
                                  z_level=0.0,
                                  color='#44ff44',
                                  label='Edge_Cuts (Top)',
                                  linewidth=2)
            self._draw_geometry_3d(self.edge_cuts_geometry,
                                  z_level=-1.6,
                                  color='#339933',
                                  label='Edge_Cuts (Bottom)',
                                  linewidth=2)
            # 側面を接続
            self._draw_board_edges(self.edge_cuts_geometry)
        
        # ドリル穴を描画 - Z = 0 から -基板厚まで
        if self.drill_data:
            self._draw_drill_holes_3d(self.drill_data)
        
        # 凡例を表示
        self.ax.legend(facecolor='#2b2b2b', edgecolor='white', 
                      labelcolor='white', loc='upper right')
        
        # ビューを設定
        self.reset_view()
        
        # 描画を更新
        self.canvas.draw()
    
    def _draw_board_surface(self):
        """基板表面を描画"""
        # バウンディングボックスを取得
        min_x, min_y, max_x, max_y = self._get_bounds()
        
        if min_x is None:
            return
        
        # 基板表面（Z=0）を半透明の平面で描画
        xx, yy = np.meshgrid([min_x, max_x], [min_y, max_y])
        zz = np.zeros_like(xx)
        
        self.ax.plot_surface(xx, yy, zz, alpha=0.1, color='green', 
                           label='Board Surface')
    
    def _draw_board_edges(self, geometry):
        """基板の側面を描画"""
        board_thickness = -1.6  # mm
        
        for line in geometry.lines:
            # 上面の線
            x_top = [line.start.x, line.end.x]
            y_top = [line.start.y, line.end.y]
            z_top = [0.0, 0.0]
            
            # 下面の線
            x_bottom = [line.start.x, line.end.x]
            y_bottom = [line.start.y, line.end.y]
            z_bottom = [board_thickness, board_thickness]
            
            # 側面を描画（縦線）
            for i in range(2):
                x_side = [x_top[i], x_bottom[i]]
                y_side = [y_top[i], y_bottom[i]]
                z_side = [z_top[i], z_bottom[i]]
                self.ax.plot(x_side, y_side, z_side, color='#226622', 
                           linewidth=1, alpha=0.3)
    
    def _draw_geometry_3d(self, geometry, z_level=0.0, color='blue', 
                         label='Geometry', linewidth=2):
        """幾何データを3Dで描画"""
        for i, line in enumerate(geometry.lines):
            self.ax.plot(
                [line.start.x, line.end.x],
                [line.start.y, line.end.y],
                [z_level, z_level],
                color=color,
                linewidth=linewidth,
                label=label if i == 0 else ""
            )
        
        # 円を描画
        for circle in geometry.circles:
            theta = np.linspace(0, 2*np.pi, 50)
            x = circle.center.x + circle.radius * np.cos(theta)
            y = circle.center.y + circle.radius * np.sin(theta)
            z = np.full_like(x, z_level)
            self.ax.plot(x, y, z, color=color, linewidth=linewidth)
    
    def _draw_drill_holes_3d(self, drill_data):
        """ドリル穴を3Dで描画（円筒形）"""
        board_thickness = -1.6  # mm
        
        for i, hole in enumerate(drill_data.holes):
            # 穴の円筒を描画
            theta = np.linspace(0, 2*np.pi, 20)
            z_hole = np.linspace(0, board_thickness, 2)
            
            # 円筒の側面
            theta_grid, z_grid = np.meshgrid(theta, z_hole)
            x_cylinder = hole.position.x + (hole.diameter / 2) * np.cos(theta_grid)
            y_cylinder = hole.position.y + (hole.diameter / 2) * np.sin(theta_grid)
            
            self.ax.plot_surface(x_cylinder, y_cylinder, z_grid,
                               color='#ff4444',
                               alpha=0.6,
                               label='Drill Holes' if i == 0 else "")
            
            # 穴の上面の円
            x_top = hole.position.x + (hole.diameter / 2) * np.cos(theta)
            y_top = hole.position.y + (hole.diameter / 2) * np.sin(theta)
            z_top = np.zeros_like(x_top)
            self.ax.plot(x_top, y_top, z_top, color='#dd0000', linewidth=1)
            
            # 穴の下面の円
            z_bottom = np.full_like(x_top, board_thickness)
            self.ax.plot(x_top, y_top, z_bottom, color='#dd0000', linewidth=1)
    
    def _get_bounds(self):
        """全データのバウンディングボックスを取得"""
        min_x = float('inf')
        min_y = float('inf')
        max_x = float('-inf')
        max_y = float('-inf')
        
        all_data = []
        if self.b_cu_geometry:
            all_data.append(self.b_cu_geometry)
        if self.edge_cuts_geometry:
            all_data.append(self.edge_cuts_geometry)
        if self.drill_data:
            all_data.append(self.drill_data)
        
        if not all_data:
            return None, None, None, None
        
        for data in all_data:
            min_point, max_point = data.get_bounds()
            min_x = min(min_x, min_point.x)
            min_y = min(min_y, min_point.y)
            max_x = max(max_x, max_point.x)
            max_y = max(max_y, max_point.y)
        
        return min_x, min_y, max_x, max_y
    
    def reset_view(self):
        """視点をリセット"""
        self.ax.view_init(elev=30, azim=45)
        self.canvas.draw()
    
    def top_view(self):
        """上面図"""
        self.ax.view_init(elev=90, azim=0)
        self.canvas.draw()
    
    def side_view(self):
        """側面図"""
        self.ax.view_init(elev=0, azim=0)
        self.canvas.draw()
