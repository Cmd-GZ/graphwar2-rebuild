# 21 · 玩法 oracle 为何不可达（证据记录）

目标第 3 项要求：为回合/目标选择/AI 建立可复现的真值来源，"拿不到就写明不可达及其证据"。
本文就是那份证据。**四条独立尝试，全部有命令输出。**

## 尝试 1 · 从 GDScript 调用原版逻辑 —— 前提不成立

把原始扩展装进真 Godot（`godot-ref/`），导出它注册的全部类：

- 12 个类、25 个方法，几乎全是 UI 回调（`on_text_change` / `on_resized` / `pressed`）；
- `AxisDrawer`、`EffectDrawer`、`GraphwarLanVBox`、`GraphwarLongPressAnimation` **零方法**；
- 没有任何方法涉及回合、关卡、AI、命中结算。

⇒ **玩法逻辑不对外**，不存在"调一个方法问出规则"的入口。详见 `docs/19` §5。

## 尝试 2 · 让真游戏跑起来再观察 —— 跑得起来但不动

```bash
./Graphwar.x86_64 --headless        # 60 秒后仍是活的（EXIT=124 是超时杀掉）
```

于是用 frida 在**启动瞬间**挂上 12 个关键函数的 hook（先轮询等 `libgraphwarrust.so`
出现再装，确保能抓到初始化调用），跑 8 秒：

```
calculate_path 0   parse_function 0   eval 0   rasterize 0   point_test 0
mask_ctor 0        turn_a 0           turn_b 0 life_entry 0   shot_calc 0
levelgen_a 0       levelgen_b 0
```

**全部零调用**。⇒ 停在主菜单时，玩法链路一次都没跑过；headless 又没有输入，
无法让它进入对局。

## 尝试 3 · 用原始场景在我们自己的项目里跑 —— 资源形态不允许

`godot-live/`：把 `rsc/` 全量 + 原始 `.so` 放进项目，主场景指向游戏自己的 `main.tscn`。

```
SCRIPT ERROR: Parse Error: Could not find type "LoadAdError" in the current scope.
ERROR: Failed to load script "res://Ad_holder.gd" with error "Parse error".
ERROR: Can't load dependency: 'res://play_screen.tscn'.
ERROR: Failed loading scene: res://main.tscn.
```

两个硬障碍：

1. **广告 SDK 是原生插件**（AdMob，Android/iOS），Linux 上不存在 → `LoadAdError`、
   `PoingAdMobVersionHelper` 等类型无定义 → `Ad_holder.gd` 解析失败 → `play_screen` 连带失败。
2. **导出态资源不可直接当源码用**：包里只有 `return.svg.import` + `.godot/imported/*.ctex`，
   **源 `.svg` 不在包内**，而场景按 `res://return.svg` 文本路径回退加载 → `Resource file not found`。

⇒ 这些资源只有**在原始 `.pck` 里**才解得开（因为 UID 缓存和 remap 都在包内）。

## 尝试 4 · 直接实例化游戏根节点 —— 同样卡在资源

在 `godot-live/`（资源齐全）里 `ClassDB.instantiate("GraphwarRootNode")`：

```
[0] 在 godot-ref（无资源）里：panic  failed to load resource at 'res://axis_option_0.svg'
[0] 在 godot-live（有资源）里：引发 play_screen 依赖链加载失败，未打印出 ROOT_INST
```

⇒ 根节点自身就要拉起完整 UI 场景，绕不开上面两个障碍。

## 结论

**在"不改游戏、不补原生插件、不伪造 UID/资源映射"的前提下，玩法逻辑没有可复现的真值来源。**
这不是没做，是被证据挡住的：

| 想要的 | 挡住它的东西 |
|---|---|
| 脚本级 oracle | 玩法不进 ClassDB |
| 运行时 hook oracle | headless 无输入，玩法不启动 |
| 自建 harness 跑原版 | 原生广告插件缺失 + 导出态资源不可源码加载 |

## 还剩下的路（成本都很高，且不一定成）

1. **给 AdMob 写桩**（提供 `LoadAdError` 等类型 + 假实现），再处理 `res://*.svg` 缺失
   （从 `.ctex` 反向生成或伪造 remap）；
2. **在真游戏进程里用 frida 调 Godot 引擎 API**（需要先在导出模板里定位
   `SceneTree::change_scene` 之类函数），把游戏推进到对局；
3. **继续纯静态逆向**，接受"只到结构"的现状。

本仓库当前的选择是 (3)，并把 (1)(2) 记录为已知可行但未验证的方向。
