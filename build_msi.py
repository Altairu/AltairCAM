import sys
import os
sys.path.insert(0, os.path.abspath('src'))
from cx_Freeze import setup, Executable

# 依存モジュールの指定
build_exe_options = {
    "packages": ["tkinter", "core", "ui", "gcode", "shapely"],
    "excludes": [],
    "include_files": ["AltairCAM.ico"]  # アイコンファイルを同梱
}

# ベースとなるGUIアプリかどうかの設定（Window非表示にするため）
base = None
if sys.platform == "win32":
    base = "Win32GUI"

setup(
    name="AltairCAM",
    version="1.0",
    description="AltairCAM PCB Milling Tool",
    options={"build_exe": build_exe_options},
    executables=[Executable("src/main.py", base=base, target_name="AltairCAM.exe", icon="AltairCAM.ico")]
)
