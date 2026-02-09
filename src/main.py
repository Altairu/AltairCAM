"""
AltairCAM メインプログラム（3ファイル対応版）

B_Cu、Edge_Cuts、Drillの3つのファイルを個別管理できるCAMソフトウェア
"""

import tkinter as tk
from tkinter import filedialog, messagebox, ttk
from pathlib import Path
from typing import Optional

from core.gerber_parser import GerberParser
from core.excellon_parser import ExcellonParser
from core.mirror import mirror_geometry, mirror_drill_data, MirrorAxis
from core.toolpath import ToolpathGenerator
from gcode.generator import GCodeGenerator
from ui.preview import PreviewCanvas
from core.geometry import Geometry, DrillData


class FileConfig:
    """各ファイルの設定を保持"""
    def __init__(self, name: str, default_tool_dia: str = "0.5", default_depth: str = "-0.1"):
        self.name = name
        self.filepath: Optional[str] = None
        self.enabled = tk.BooleanVar(value=True)
        self.mirror_axis = tk.StringVar(value="none")
        self.tool_diameter = tk.StringVar(value=default_tool_dia)
        self.cut_depth = tk.StringVar(value=default_depth)
        self.feed_rate = tk.StringVar(value="100")
        self.optimize_toolpath = tk.BooleanVar(value=False)  # ツールパス最適化
        self.data = None  # Geometry または DrillData


class AltairCAMApp:
    """AltairCAM メインアプリケーション"""
    
    def __init__(self, root):
        self.root = root
        self.root.title("AltairCAM - PCB CNC Tool")
        self.root.geometry("1600x900")
        
        # 3つのファイル設定
        self.b_cu_config = FileConfig("B_Cu", default_tool_dia="0.1", default_depth="-0.05")
        self.edge_cuts_config = FileConfig("Edge_Cuts", default_tool_dia="1.0", default_depth="-1.6")
        self.drill_config = FileConfig("Drill", default_tool_dia="0.8", default_depth="-1.7")
        
        # プレビューキャンバス
        self.preview_canvas: Optional[PreviewCanvas] = None
        
        # メニューバーを構築
        self._build_menu()
        
        # UI構築
        self._build_ui()
    
    def _build_menu(self):
        """メニューバーを構築"""
        from ui.help_dialog import HelpDialog
        
        menubar = tk.Menu(self.root)
        self.root.config(menu=menubar)
        
        # ファイルメニュー
        file_menu = tk.Menu(menubar, tearoff=0)
        menubar.add_cascade(label="ファイル", menu=file_menu)
        file_menu.add_command(label="すべて読み込み", command=self._load_all_files)
        file_menu.add_separator()
        file_menu.add_command(label="終了", command=self.root.quit)
        
        # ツールメニュー
        tools_menu = tk.Menu(menubar, tearoff=0)
        menubar.add_cascade(label="ツール", menu=tools_menu)
        tools_menu.add_command(label="プレビュー更新", command=self._update_preview)
        tools_menu.add_separator()
        tools_menu.add_command(label="3Dプレビュー", command=self._show_3d_preview)
        tools_menu.add_command(label="切削シミュレーション", command=self._show_simulation)
        
        # ヘルプメニュー
        help_menu = tk.Menu(menubar, tearoff=0)
        menubar.add_cascade(label="ヘルプ", menu=help_menu)
        help_menu.add_command(label="使い方", command=lambda: HelpDialog.show_usage_guide(self.root))
        help_menu.add_command(label="ショートカットキー", command=lambda: HelpDialog.show_shortcuts(self.root))
        help_menu.add_separator()
        help_menu.add_command(label="バージョン情報", command=lambda: HelpDialog.show_about(self.root))
    
    def _show_3d_preview(self):
        """3Dプレビューを表示"""
        # データが読み込まれているか確認
        if not (self.b_cu_config.data or self.edge_cuts_config.data or self.drill_config.data):
            messagebox.showwarning("警告", "まずファイルを読み込んでください")
            return
        
        try:
            from ui.preview_3d import Preview3DWindow
            
            # 3Dプレビューウィンドウを開く
            Preview3DWindow(
                self.root,
                b_cu_geometry=self.b_cu_config.data if self.b_cu_config.enabled.get() else None,
                b_cu_tool_diameter=float(self.b_cu_config.tool_diameter.get()) if self.b_cu_config.data else 0.5,
                edge_cuts_geometry=self.edge_cuts_config.data if self.edge_cuts_config.enabled.get() else None,
                edge_cuts_tool_diameter=float(self.edge_cuts_config.tool_diameter.get()) if self.edge_cuts_config.data else 0.5,
                drill_data=self.drill_config.data if self.drill_config.enabled.get() else None,
                drill_tool_diameter=float(self.drill_config.tool_diameter.get()) if self.drill_config.data else 0.8
            )
        except Exception as e:
            messagebox.showerror("エラー", f"3Dプレビューの表示に失敗しました:\n{str(e)}")
    
    def _show_simulation(self):
        """切削シミュレーションを表示"""
        try:
            from ui.simulation_window import SimulationWindow
            
            # シミュレーションウィンドウを開く
            SimulationWindow(self.root)
        except Exception as e:
            messagebox.showerror("エラー", f"シミュレーションの表示に失敗しました:\n{str(e)}")
    
    def _build_ui(self):
        """UIを構築"""
        # メインコンテナ（左右分割）
        main_container = ttk.PanedWindow(self.root, orient=tk.HORIZONTAL)
        main_container.pack(fill=tk.BOTH, expand=True)
        
        # 左側：コントロールパネル（スクロール可能）
        left_panel = ttk.Frame(main_container, width=700)
        main_container.add(left_panel, weight=0)
        
        # スクロール可能なキャンバス
        canvas = tk.Canvas(left_panel, width=680)
        scrollbar = ttk.Scrollbar(left_panel, orient="vertical", command=canvas.yview)
        scrollable_frame = ttk.Frame(canvas)
        
        scrollable_frame.bind(
            "<Configure>",
            lambda e: canvas.configure(scrollregion=canvas.bbox("all"))
        )
        
        canvas.create_window((0, 0), window=scrollable_frame, anchor="nw")
        canvas.configure(yscrollcommand=scrollbar.set)
        
        canvas.pack(side="left", fill="both", expand=True)
        scrollbar.pack(side="right", fill="y")
        
        # 右側：プレビューパネル
        right_panel = ttk.Frame(main_container)
        main_container.add(right_panel, weight=1)
        
        # === 左側パネルの構築（スクロール可能フレーム内）===
        
        # B_Cuファイルセクション
        self._build_file_section(scrollable_frame, self.b_cu_config, "B_Cu (銅箔層)", 
                                 filetypes=[("Gerber files", "*.gbr *.GBR"), ("All files", "*.*")])
        
        # Edge_Cutsファイルセクション
        self._build_file_section(scrollable_frame, self.edge_cuts_config, "Edge_Cuts (基板外形)",
                                 filetypes=[("Gerber files", "*.gbr *.GBR"), ("All files", "*.*")])
        
        # ドリルファイルセクション
        self._build_file_section(scrollable_frame, self.drill_config, "Drill (ドリル穴)",
                                 filetypes=[("Drill files", "*.drl *.DRL *.txt"), ("All files", "*.*")])
        
        # グローバル操作ボタン
        global_frame = ttk.Frame(scrollable_frame, padding="10")
        global_frame.pack(fill=tk.X)
        
        ttk.Button(global_frame, text="📂 すべて読み込み", 
                  command=self._load_all_files).pack(side=tk.LEFT, padx=5)
        ttk.Button(global_frame, text="🔄 プレビュー更新", 
                  command=self._update_preview).pack(side=tk.LEFT, padx=5)
        
        # ログフレーム
        log_frame = ttk.LabelFrame(scrollable_frame, text="ログ", padding="10")
        log_frame.pack(fill=tk.BOTH, expand=True, padx=10, pady=5)
        
        self.log_text = tk.Text(log_frame, height=8, width=70)
        self.log_text.pack(fill=tk.BOTH, expand=True)
        
        # スクロールバー
        log_scrollbar = ttk.Scrollbar(log_frame, command=self.log_text.yview)
        log_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.log_text.config(yscrollcommand=log_scrollbar.set)
        
        # === 右側パネルの構築：プレビュー ===
        
        preview_label_frame = ttk.LabelFrame(right_panel, text="2D Preview", padding="10")
        preview_label_frame.pack(fill=tk.BOTH, expand=True)
        
        # プレビューキャンバスを作成
        self.preview_canvas = PreviewCanvas(preview_label_frame, width=11, height=9)
        self.preview_canvas.pack(fill=tk.BOTH, expand=True)
    
    def _build_file_section(self, parent, config: FileConfig, title: str, filetypes: list):
        """各ファイルのセクションを構築"""
        frame = ttk.LabelFrame(parent, text=title, padding="10")
        frame.pack(fill=tk.X, padx=10, pady=5)
        
        # 有効/無効チェックボックス
        ttk.Checkbutton(frame, text="有効", variable=config.enabled,
                       command=self._update_preview).grid(row=0, column=0, sticky=tk.W)
        
        # ファイル選択
        ttk.Label(frame, text="ファイル:").grid(row=1, column=0, sticky=tk.W, pady=3)
        file_entry = ttk.Entry(frame, width=45)
        file_entry.grid(row=1, column=1, padx=5, pady=3)
        ttk.Button(frame, text="参照...", 
                  command=lambda: self._browse_file(config, file_entry, filetypes)).grid(row=1, column=2, pady=3)
        
        # 反転軸
        ttk.Label(frame, text="反転軸:").grid(row=2, column=0, sticky=tk.W, pady=3)
        mirror_frame = ttk.Frame(frame)
        mirror_frame.grid(row=2, column=1, columnspan=2, sticky=tk.W, pady=3)
        ttk.Radiobutton(mirror_frame, text="なし", variable=config.mirror_axis, value="none").pack(side=tk.LEFT)
        ttk.Radiobutton(mirror_frame, text="X軸", variable=config.mirror_axis, value="x").pack(side=tk.LEFT, padx=5)
        ttk.Radiobutton(mirror_frame, text="Y軸", variable=config.mirror_axis, value="y").pack(side=tk.LEFT)
        
        # パラメータ（2列レイアウト）
        param_frame = ttk.Frame(frame)
        param_frame.grid(row=3, column=0, columnspan=3, sticky=tk.W, pady=3)
        
        ttk.Label(param_frame, text="ツール直径:").grid(row=0, column=0, sticky=tk.W, padx=(0,5))
        ttk.Entry(param_frame, textvariable=config.tool_diameter, width=8).grid(row=0, column=1, sticky=tk.W)
        ttk.Label(param_frame, text="mm").grid(row=0, column=2, sticky=tk.W, padx=(2,15))
        
        ttk.Label(param_frame, text="切削深さ:").grid(row=0, column=3, sticky=tk.W, padx=(0,5))
        ttk.Entry(param_frame, textvariable=config.cut_depth, width=8).grid(row=0, column=4, sticky=tk.W)
        ttk.Label(param_frame, text="mm").grid(row=0, column=5, sticky=tk.W, padx=(2,15))
        
        ttk.Label(param_frame, text="送り速度:").grid(row=1, column=0, sticky=tk.W, padx=(0,5), pady=(5,0))
        ttk.Entry(param_frame, textvariable=config.feed_rate, width=8).grid(row=1, column=1, sticky=tk.W, pady=(5,0))
        ttk.Label(param_frame, text="mm/min").grid(row=1, column=2, sticky=tk.W, padx=(2,15), pady=(5,0))
        
        # ツールパス最適化（ドリルのみ）
        if config.name == "Drill":
            ttk.Checkbutton(frame, text="✨ ツールパス最適化（移動距離を最小化）", 
                          variable=config.optimize_toolpath).grid(row=4, column=0, columnspan=3, sticky=tk.W, pady=3)
        
        # Gコード生成ボタン
        button_row = 5 if config.name == "Drill" else 4
        ttk.Button(frame, text="📝 Gコード生成", 
                  command=lambda: self._generate_gcode_for_file(config)).grid(row=button_row, column=0, columnspan=3, pady=5)
    
    def _browse_file(self, config: FileConfig, entry: ttk.Entry, filetypes: list):
        """ファイルを参照"""
        filename = filedialog.askopenfilename(
            title=f"{config.name}ファイルを選択",
            filetypes=filetypes
        )
        if filename:
            entry.delete(0, tk.END)
            entry.insert(0, filename)
            config.filepath = filename
    
    def _log(self, message: str):
        """ログに出力"""
        self.log_text.insert(tk.END, message + "\n")
        self.log_text.see(tk.END)
    
    def _load_all_files(self):
        """すべてのファイルを読み込み"""
        try:
            # まず反転なしで全データを読み込み
            b_cu_data = None
            edge_cuts_data = None
            drill_data = None
            
            # B_Cuを読み込み（反転なし）
            if self.b_cu_config.enabled.get() and self.b_cu_config.filepath:
                self._log(f"B_Cuを読み込み中: {self.b_cu_config.filepath}")
                parser = GerberParser()
                b_cu_data = parser.parse_file(self.b_cu_config.filepath)
                self._log(f"  -> {b_cu_data}")
            
            # Edge_Cutsを読み込み（反転なし）
            if self.edge_cuts_config.enabled.get() and self.edge_cuts_config.filepath:
                self._log(f"Edge_Cutsを読み込み中: {self.edge_cuts_config.filepath}")
                parser = GerberParser()
                edge_cuts_data = parser.parse_file(self.edge_cuts_config.filepath)
                self._log(f"  -> {edge_cuts_data}")
            
            # ドリルを読み込み（反転なし）
            if self.drill_config.enabled.get() and self.drill_config.filepath:
                self._log(f"Drillを読み込み中: {self.drill_config.filepath}")
                parser = ExcellonParser()
                drill_data = parser.parse_file(self.drill_config.filepath)
                self._log(f"  -> {drill_data}")
            
            # 統合バウンディングボックスを計算（反転基準点の決定）
            all_data = []
            if b_cu_data:
                all_data.append(b_cu_data)
            if edge_cuts_data:
                all_data.append(edge_cuts_data)
            if drill_data:
                all_data.append(drill_data)
            
            if all_data:
                # 全データの統合バウンディングボックスを取得
                min_x = float('inf')
                min_y = float('inf')
                max_x = float('-inf')
                max_y = float('-inf')
                
                for data in all_data:
                    min_point, max_point = data.get_bounds()
                    min_x = min(min_x, min_point.x)
                    min_y = min(min_y, min_point.y)
                    max_x = max(max_x, max_point.x)
                    max_y = max(max_y, max_point.y)
                
                # 統合された中心点を計算
                center_x = (min_x + max_x) / 2
                center_y = (min_y + max_y) / 2
                
                self._log(f"統合バウンディングボックス: ({min_x:.2f}, {min_y:.2f}) - ({max_x:.2f}, {max_y:.2f})")
                self._log(f"反転基準点: X={center_x:.2f}, Y={center_y:.2f}")
                
                # 各データを同じ基準点で反転
                # B_Cu
                if b_cu_data:
                    mirror_axis = self.b_cu_config.mirror_axis.get()
                    if mirror_axis != "none":
                        axis = MirrorAxis.X if mirror_axis == "x" else MirrorAxis.Y
                        reference = center_y if axis == MirrorAxis.X else center_x
                        self._log(f"B_Cuを{mirror_axis.upper()}軸で反転中（基準: {reference:.2f}）...")
                        b_cu_data = mirror_geometry(b_cu_data, axis, reference)
                    self.b_cu_config.data = b_cu_data
                
                # Edge_Cuts
                if edge_cuts_data:
                    mirror_axis = self.edge_cuts_config.mirror_axis.get()
                    if mirror_axis != "none":
                        axis = MirrorAxis.X if mirror_axis == "x" else MirrorAxis.Y
                        reference = center_y if axis == MirrorAxis.X else center_x
                        self._log(f"Edge_Cutsを{mirror_axis.upper()}軸で反転中（基準: {reference:.2f}）...")
                        edge_cuts_data = mirror_geometry(edge_cuts_data, axis, reference)
                    self.edge_cuts_config.data = edge_cuts_data
                
                # Drill
                if drill_data:
                    mirror_axis = self.drill_config.mirror_axis.get()
                    if mirror_axis != "none":
                        axis = MirrorAxis.X if mirror_axis == "x" else MirrorAxis.Y
                        reference = center_y if axis == MirrorAxis.X else center_x
                        self._log(f"Drillを{mirror_axis.upper()}軸で反転中（基準: {reference:.2f}）...")
                        drill_data = mirror_drill_data(drill_data, axis, reference)
                    self.drill_config.data = drill_data
            
            self._log("ファイルの読み込みが完了しました")
            
            # プレビューを更新
            self._update_preview()
            
        except Exception as e:
            messagebox.showerror("エラー", f"ファイルの読み込みに失敗しました:\n{str(e)}")
            self._log(f"エラー: {str(e)}")
    
    def _update_preview(self):
        """プレビューを更新"""
        if self.preview_canvas:
            self._log("プレビューを更新中...")
            
            # 各ファイルのデータとツール直径を取得
            b_cu_geo = self.b_cu_config.data if self.b_cu_config.enabled.get() else None
            b_cu_dia = float(self.b_cu_config.tool_diameter.get()) if b_cu_geo else 0.5
            
            edge_geo = self.edge_cuts_config.data if self.edge_cuts_config.enabled.get() else None
            edge_dia = float(self.edge_cuts_config.tool_diameter.get()) if edge_geo else 0.5
            
            drill_data = self.drill_config.data if self.drill_config.enabled.get() else None
            drill_dia = float(self.drill_config.tool_diameter.get()) if drill_data else 0.8
            
            self.preview_canvas.update_preview(
                b_cu_geometry=b_cu_geo,
                b_cu_tool_diameter=b_cu_dia,
                edge_cuts_geometry=edge_geo,
                edge_cuts_tool_diameter=edge_dia,
                drill_data=drill_data,
                drill_tool_diameter=drill_dia
            )
            self._log("プレビュー更新完了")
    
    def _generate_gcode_for_file(self, config: FileConfig):
        """特定のファイルのGコードを生成"""
        if not config.data:
            messagebox.showwarning("警告", f"{config.name}は読み込まれていません")
            return
        
        if not config.enabled.get():
            messagebox.showwarning("警告", f"{config.name}は無効になっています")
            return
        
        try:
            # パラメータを取得
            tool_diameter = float(config.tool_diameter.get())
            cut_depth = float(config.cut_depth.get())
            feed_rate = float(config.feed_rate.get())
            
            self._log(f"{config.name}のGコードを生成中...")
            
            # ツールパス生成
            toolpath_gen = ToolpathGenerator()
            toolpath_gen.tool_diameter = tool_diameter
            
            # Gコード生成器
            gcode_gen = GCodeGenerator()
            gcode_gen.cut_z = cut_depth
            gcode_gen.feed_rate = feed_rate
            
            all_gcode = []
            all_gcode.extend(gcode_gen.generate_header())
            
            # データの種類に応じてGコード生成
            if isinstance(config.data, Geometry):
                # ガーバーデータ: アイソレーションルーティング
                self._log("  アイソレーションルーティングを生成中...")
                toolpaths = toolpath_gen.generate_isolation_routing(config.data)
                for path in toolpaths:
                    all_gcode.extend(gcode_gen.generate_line_path(path))
            
            elif isinstance(config.data, DrillData):
                # ドリルデータ
                self._log("  ドリルパスを生成中...")
                
                # 最適化オプションを取得
                optimize = config.optimize_toolpath.get() if hasattr(config, 'optimize_toolpath') else False
                
                if optimize:
                    # 最適化前後の比較情報を表示
                    from core.optimizer import ToolpathOptimizer
                    optimizer = ToolpathOptimizer()
                    comparison = optimizer.compare_optimization(config.data)
                    self._log(f"  最適化前の移動距離: {comparison['original_distance']:.2f} mm")
                    self._log(f"  最適化後の移動距離: {comparison['optimized_distance']:.2f} mm")
                    self._log(f"  改善率: {comparison['improvement_percent']:.1f}%")
                
                # ツールパス生成（最適化適用）
                drill_path = toolpath_gen.generate_drill_toolpath(config.data, optimize_order=optimize)
                all_gcode.extend(gcode_gen.generate_drill_path(drill_path))
            
            all_gcode.extend(gcode_gen.generate_footer())
            
            # ファイルに保存
            output_file = filedialog.asksaveasfilename(
                title=f"{config.name}のGコードを保存",
                defaultextension=".nc",
                initialfile=f"{config.name.replace(' ', '_').lower()}.nc",
                filetypes=[("G-Code files", "*.nc *.NC *.gcode"), ("All files", "*.*")]
            )
            
            if output_file:
                gcode_gen.save_to_file(all_gcode, output_file)
                self._log(f"Gコードを保存しました: {output_file}")
                self._log(f"総行数: {len(all_gcode)}")
                messagebox.showinfo("完了", f"Gコードの生成が完了しました:\n{output_file}")
            
        except Exception as e:
            messagebox.showerror("エラー", f"Gコードの生成に失敗しました:\n{str(e)}")
            self._log(f"エラー: {str(e)}")


def main():
    """メイン関数"""
    root = tk.Tk()
    app = AltairCAMApp(root)
    root.mainloop()


if __name__ == "__main__":
    main()
