"""
切削シミュレーションウィンドウ

Gコードを読み込んでアニメーション表示
"""

import tkinter as tk
from tkinter import filedialog, messagebox, ttk
import matplotlib.pyplot as plt
from matplotlib.backends.backend_tkagg import FigureCanvasTkAgg, NavigationToolbar2Tk
from mpl_toolkits.mplot3d import Axes3D
from matplotlib.figure import Figure
from matplotlib.animation import FuncAnimation
import numpy as np

from ui.simulator import GCodeSimulator
from core.geometry import Point


class SimulationWindow:
    """切削シミュレーションウィンドウ"""
    
    def __init__(self, parent):
        """
        切削シミュレーションウィンドウを初期化
        
        Args:
            parent: 親ウィンドウ
        """
        self.window = tk.Toplevel(parent)
        self.window.title("Cutting Simulation - AltairCAM")
        self.window.geometry("1200x900")
        
        # シミュレーター
        self.simulator = GCodeSimulator()
        self.toolpaths = []
        self.current_frame = 0
        self.is_playing = False
        self.animation = None
        self.speed = 1.0  # 再生速度倍率
        
        # ツール位置を追跡する線のリスト
        self.path_lines = []
        
        # 3Dプロット用のフィギュアを作成
        self.fig = Figure(figsize=(12, 8), facecolor='#2b2b2b')
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
        self._create_controls()
        
        # 初期化
        self._setup_plot()
    
    def _create_controls(self):
        """コントロールパネルを作成"""
        control_frame = ttk.Frame(self.window, padding="10")
        control_frame.pack(fill=tk.X)
        
        # ファイル読み込みボタン
        ttk.Button(control_frame, text="📂 Gコードファイルを開く", 
                  command=self.load_gcode).pack(side=tk.LEFT, padx=5)
        
        # 再生コントロール
        ttk.Separator(control_frame, orient=tk.VERTICAL).pack(side=tk.LEFT, padx=10, fill=tk.Y)
        
        self.play_button = ttk.Button(control_frame, text="▶ 再生", 
                                     command=self.play_simulation)
        self.play_button.pack(side=tk.LEFT, padx=5)
        self.play_button.config(state=tk.DISABLED)
        
        self.pause_button = ttk.Button(control_frame, text="⏸ 一時停止", 
                                      command=self.pause_simulation)
        self.pause_button.pack(side=tk.LEFT, padx=5)
        self.pause_button.config(state=tk.DISABLED)
        
        ttk.Button(control_frame, text="⏹ 停止", 
                  command=self.stop_simulation).pack(side=tk.LEFT, padx=5)
        
        # 速度調整
        ttk.Separator(control_frame, orient=tk.VERTICAL).pack(side=tk.LEFT, padx=10, fill=tk.Y)
        
        ttk.Label(control_frame, text="速度:").pack(side=tk.LEFT, padx=(0, 5))
        
        self.speed_var = tk.StringVar(value="1.0")
        speed_combo = ttk.Combobox(control_frame, textvariable=self.speed_var, 
                                   values=["0.5", "1.0", "2.0", "5.0", "10.0"],
                                   width=8, state="readonly")
        speed_combo.pack(side=tk.LEFT, padx=5)
        speed_combo.bind("<<ComboboxSelected>>", self._on_speed_change)
        
        ttk.Label(control_frame, text="x").pack(side=tk.LEFT)
        
        # 進捗表示
        ttk.Separator(control_frame, orient=tk.VERTICAL).pack(side=tk.LEFT, padx=10, fill=tk.Y)
        
        self.progress_label = ttk.Label(control_frame, text="0 / 0")
        self.progress_label.pack(side=tk.LEFT, padx=5)
        
        # 閉じるボタン
        ttk.Button(control_frame, text="閉じる", 
                  command=self.window.destroy).pack(side=tk.RIGHT, padx=5)
    
    def _setup_plot(self):
        """プロットの初期設定"""
        self.ax.clear()
        
        # 軸ラベル設定
        self.ax.set_xlabel('X (mm)', color='white')
        self.ax.set_ylabel('Y (mm)', color='white')
        self.ax.set_zlabel('Z (mm)', color='white')
        self.ax.set_title('Cutting Simulation', color='white', fontsize=14, pad=20)
        
        # グリッド設定
        self.ax.grid(True, alpha=0.3)
        self.ax.xaxis.pane.fill = False
        self.ax.yaxis.pane.fill = False
        self.ax.zaxis.pane.fill = False
        
        # 軸の色を設定
        self.ax.tick_params(colors='white')
        
        # 基板表面（Z=0）
        self.ax.plot([0, 50], [0, 0], [0, 0], color='green', alpha=0.3)
        
        # ビューを設定
        self.ax.view_init(elev=30, azim=45)
        
        self.canvas.draw()
    
    def load_gcode(self):
        """Gコードファイルを読み込み"""
        filename = filedialog.askopenfilename(
            title="Gコードファイルを選択",
            filetypes=[("G-Code files", "*.nc *.NC *.gcode *.GCODE"), ("All files", "*.*")]
        )
        
        if not filename:
            return
        
        try:
            # Gコードを解析
            self.toolpaths = self.simulator.load_gcode_file(filename)
            
            if not self.toolpaths:
                messagebox.showwarning("警告", "有効なツールパスが見つかりませんでした")
                return
            
            messagebox.showinfo("成功", f"{len(self.toolpaths)}個のツールパスを読み込みました")
            
            # プロットを初期化
            self._setup_simulation()
            
            # 再生ボタンを有効化
            self.play_button.config(state=tk.NORMAL)
            
        except Exception as e:
            messagebox.showerror("エラー", f"Gコードの読み込みに失敗しました:\n{str(e)}")
    
    def _setup_simulation(self):
        """シミュレーションの準備"""
        self.current_frame = 0
        self.is_playing = False
        self.path_lines = []
        
        # プロットをクリア
        self.ax.clear()
        
        # 軸設定
        self.ax.set_xlabel('X (mm)', color='white')
        self.ax.set_ylabel('Y (mm)', color='white')
        self.ax.set_zlabel('Z (mm)', color='white')
        self.ax.set_title('Cutting Simulation', color='white', fontsize=14, pad=20)
        self.ax.grid(True, alpha=0.3)
        self.ax.xaxis.pane.fill = False
        self.ax.yaxis.pane.fill = False
        self.ax.zaxis.pane.fill = False
        self.ax.tick_params(colors='white')
        
        # バウンディングボックスを計算
        all_points = []
        for start, end, _ in self.toolpaths:
            all_points.extend([start, end])
        
        if all_points:
            xs = [p.x for p in all_points]
            ys = [p.y for p in all_points]
            zs = [p.z for p in all_points]
            
            self.ax.set_xlim(min(xs) - 5, max(xs) + 5)
            self.ax.set_ylim(min(ys) - 5, max(ys) + 5)
            self.ax.set_zlim(min(zs) - 5, 5)
        
        # ビューを設定
        self.ax.view_init(elev=30, azim=45)
        
        # 進捗を更新
        self.progress_label.config(text=f"0 / {len(self.toolpaths)}")
        
        self.canvas.draw()
    
    def play_simulation(self):
        """シミュレーションを再生"""
        if not self.toolpaths:
            return
        
        self.is_playing = True
        self.play_button.config(state=tk.DISABLED)
        self.pause_button.config(state=tk.NORMAL)
        
        # アニメーションを開始
        if self.animation is None or not self.animation.event_source:
            # 速度に応じてインターバルを調整（ミリ秒）
            interval = max(1, int(20 / self.speed))
            self.animation = FuncAnimation(self.fig, self._update_frame, 
                                          frames=len(self.toolpaths),
                                          interval=interval,
                                          repeat=False,
                                          blit=False)
            self.canvas.draw()
    
    def pause_simulation(self):
        """シミュレーションを一時停止"""
        self.is_playing = False
        self.play_button.config(state=tk.NORMAL)
        self.pause_button.config(state=tk.DISABLED)
        
        if self.animation:
            self.animation.event_source.stop()
    
    def stop_simulation(self):
        """シミュレーションを停止"""
        self.is_playing = False
        self.current_frame = 0
        
        if self.animation:
            self.animation.event_source.stop()
            self.animation = None
        
        self.play_button.config(state=tk.NORMAL)
        self.pause_button.config(state=tk.DISABLED)
        
        # プロットをリセット
        if self.toolpaths:
            self._setup_simulation()
    
    def _on_speed_change(self, event):
        """速度が変更されたとき"""
        self.speed = float(self.speed_var.get())
        
        # 再生中の場合はアニメーションを再開
        if self.is_playing and self.animation:
            self.pause_simulation()
            self.play_simulation()
    
    def _update_frame(self, frame):
        """フレームを更新（アニメーションコールバック）"""
        if not self.is_playing or frame >= len(self.toolpaths):
            return
        
        self.current_frame = frame
        
        # 現在のツールパス
        start, end, path_type = self.toolpaths[frame]
        
        # 色を決定
        if path_type == 'drill':
            color = '#ff4444'
            linewidth = 3
        elif path_type == 'cut':
            color = '#44ff44'
            linewidth = 2
        else:  # rapid
            color = '#4488ff'
            linewidth = 1
        
        # 線を描画
        line, = self.ax.plot(
            [start.x, end.x],
            [start.y, end.y],
            [start.z, end.z],
            color=color,
            linewidth=linewidth,
            alpha=0.7
        )
        self.path_lines.append(line)
        
        # 進捗を更新
        self.progress_label.config(text=f"{frame + 1} / {len(self.toolpaths)}")
        
        # 描画を更新
        self.canvas.draw_idle()
        
        # 最後のフレームに達したら停止
        if frame >= len(self.toolpaths) - 1:
            self.is_playing = False
            self.play_button.config(state=tk.NORMAL)
            self.pause_button.config(state=tk.DISABLED)
