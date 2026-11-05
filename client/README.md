# Graphwar II —— Godot 客户端重建（`client/`）

这是**我们自己的** Godot 4.7.2 客户端工程，用来补上重建仓库里"界面实现进度 = 0"的缺口。

* **不含任何游戏资产**：没有从 `Graphwar.pck` 解出来的 `.tscn` / `.gd` / `.ttf` / `.svg`，
  也没有字体文件（用本机系统字体，见 `src/ui/theme_factory.gd`）。
* 15 个界面全部到位（主菜单 / 战役 / 联机大厅 / 对局设置 / 对局 / 玩家 / 回放 / 8 个弹窗），
  可以启动、可以互相跳转、可以真的开一局（弹道会画出来）。
* 界面背后的**规则**仍然归 Rust 内核（`../src/`）。本工程通过 `src/core/core_bridge.gd`
  这一层接缝调用它；内核目前只暴露了 `GW2Rebuild.ping()`，所以弹道暂时走"界面预览"。

架构、已实现/占位/未做的清单、字体与图标的处理方式、以及怎么跑，
都在 **[`../docs/108-client-rebuild.md`](../docs/108-client-rebuild.md)**。

## 最快上手

```bash
# 1) 首次需要让 Godot 扫一遍资源（生成 .godot/ 与 global_script_class_cache.cfg）
../../../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path . --import

# 2) 启动（有显示器）
../../../../tools/Godot_v4.7.2-stable_linux.x86_64 --path .

# 3) 无显示器时的验收
bash run.sh selftest     # 93 条断言，退出码 0/1
bash run.sh dump         # 打印整棵节点树（格式仿 docs/92 的 @@N 行）
bash run.sh shots        # 逐界面截图（需要 Xvfb）
```
