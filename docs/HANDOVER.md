# HANDOVER —— 给上下文压缩后的自己

**读这一篇就够了。** 它替代"翻 17 篇文档"的过程。若与本篇冲突，以代码与 `docs/` 原文为准。

最后更新：**Godot 4.7.2 已就位，扩展能在真引擎里加载/注册/被调用；
用还原资源打包的 `.pck` 已能启动**。

### 当前可复现的三条验收（真引擎）

```bash
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
# ① 扩展加载 + 类注册 + 方法调用
cd godot-probe && ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --editor --path . --quit
                   ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path .
#   期望：GW2_HAS_CLASS/INSTANCE/HAS_PING=true，并打印 "ping() called from GDScript"
# ② 用还原资源打包并从 pck 启动
cd godot-dist && ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --main-pack out.pck
#   期望：GW2_TEX=true size=(1024.0, 1016.0)
# ③ 原始扩展的 API 面（负面结论：只暴露 12 类 25 个 UI 方法）
cd godot-ref && ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path .
```

**注意**：`~` 只读，必须设三个 XDG 目录；`godot-probe/main.tscn` 必须指向 `main.gd`
（指错会静默退出，什么都不打印）。

### 活体 oracle（本轮新增，能力级突破）

```bash
node tools/frida_live.js modules
node tools/frida_live.js call <rva-hex> <ret-type> [type:value,...]
```

frida 拉起真游戏 → 等扩展加载 → 在**运行中的进程**里调 `libgraphwarrust.so` 的内部函数。
实测 `0x9302b0`（Ghidra `0xa302b0`）：`2.7→2`、`-2.7→-3` ⇒ 是 **floor**。
详见 `docs/20-live-oracle.md`。

前置修复（一次性）：`cd .rea-runtime && npm install-scripts approve frida &&
npm rebuild frida --foreground-scripts`。REA 自带 frida 工具因缓存失效仍不可用，
所以直接走 Node API。**每次 bash 是独立 PID 命名空间**，必须用 spawn 模式。

### 玩法 oracle：已尝试四条路，全部有证据地失败

见 `docs/21-oracle-limits.md`。摘要：
1. GDScript 层面玩法不暴露（12 类 25 个 UI 方法）；
2. headless 主菜单下 12 个玩法函数 hook **全零调用**；
3. 自建项目跑原版场景失败：AdMob 原生插件缺失 + 导出态资源（只有 `.ctex`/`.import`，
   源 `.svg` 不在包内）无法按源码路径加载；
4. 直接实例化 `GraphwarRootNode`：无资源时 panic，有资源时依赖链仍失败。

⇒ 回合/关卡/AI/结算保持 🔶/⬜ 是**缺真值**，不是没做。

### 本轮新增的两个坑

7. `GDExtensionClassMethodInfo` 末尾还有 `default_argument_count` / `default_arguments`，
   少写这两个字段 → 引擎 abort（读到栈外垃圾）。
8. `.pck` **不包含** `.so`：`res://libgw2_rebuild.so` 会 `file not found`，
   必须用裸文件名并把库放在 `.pck` 同目录。

**四条独立验收**（都能重跑）：
```
cargo test                        40 条通过
cargo build --release             产出 libgw2_rebuild.so (ELF, 12 个导出符号)
tools/gdext_probe.c               dlopen + 入口契约 PASS
tools/{capi,mask,mask_api,simulate}_check.py   全部 RESULT: PASS
```

---

## 1. 现在在哪

| 仓库 | 远端 | 状态 |
|---|---|---|
| `gw2-rebuild`（本仓库） | `Cmd-GZ/graphwar2` | Rust 复原 21 条测试全过；`rsc/` 已还原但 gitignore |
| `gw2`（上游） | `Cmd-GZ/graphwar-algorithm-analysis` | 曲线算法报告已更正；与本仓库互不包含 |

两仓库都只允许用 `GIT_SSH_COMMAND="ssh -F /dev/null" git push`（本机 ssh 配置有权限问题）。

## 2. 已完成、有验收的部分

| 部件 | 位置 | 验收 |
|---|---|---|
| 函数文法（词法/解析/求值） | `src/function/` | 37 + 70 条真值夹具 |
| 等弦长采点（含 RK4） | `src/path.rs` | 224 条（flip × ode 全覆盖） |
| 修正常数 + 端到端 + **开火模拟原语** | `src/shot.rs` | 端到端 + 上游 648 例；装配校验 15 组（含 ODE/flip/2 次真实命中）`tools/simulate_check.py` |
| 坐标变换 + 掩膜点测试 + **光栅化** | `src/plane.rs` | 14 条 + 与 oracle 逐字节对拍 |
| 事件枚举形状 + 生命两张表 | `src/events.rs`、`src/life.rs` | 5 条 |
| 游戏资源 | `tools/pck_extract.py` → `rsc/` | 692 文件 md5 全符 |
| **可构建二进制** | `Cargo.toml` cdylib + `src/gdext.rs` | `cargo build --release` 出 .so；`nm` 见 `gdext_rust_init`；`tools/gdext_probe.c` dlopen 实测 PASS |

## 3. 核心结论速查（省掉重新推导）

**曲线**（逐位验证）
`parse @0x4ed670` / `eval @0x4eb9d0` / `path @0x3c2530`。
`step=0.01`、容差 `0.001`、最多 100 次二分、点数 `(200/step) as i32`、
边界 `|x|≤25 |y|≤15`、修正常数 `c = y0 − f(±x0)`。

**坐标**（观测，`0x4be3e0`）
`px = 20x + 500`、`py = 20y + 300`，画布 1000×600。
三处（`0x4be3e0`/`0x4f11e0`/`0x4f15b0`）同构，`.so` 内**无 y 翻转**。

**命中**（观测，`0x4f15b0`）
1000×600 字节掩膜，`idx = py*1000+px`，`!= 0xFF` 即命中，**越界算命中**。
掩膜由 `obstacle_manager` 的 `0x523020` 创建：`alloc(600000)` + `memset 0xFF`
+ 引用计数容器；同模块负责填充（`0x4f11e0`）与查询（`0x4f15b0`）。
**三个 API 都已与真二进制对拍通过**（`tools/mask_api_check.py`，随机用例 0 差异）。

**资源**（实测）
PCK v4：索引在包尾（头部 `0x20` 处 `directory_offset`），
偏移基准是 `file_base`（**不是**包起点，flags=2 会骗人）。

**控制流**（观测）
`calculate_path` ← `shot_calculator`、`target_manager`；
`shot_calculator` ← `game_state_manager`、`function_evolutionator`（均经函数指针）。

## 4. 明确未解决（别假装做了）

| 块 | 卡点 |
|---|---|
| ~~光栅化~~ | **已完成**：`src/plane.rs::rasterize` 与真二进制逐字节一致（`tools/mask_check.py`，5 组 0 差异） |
| 回合 `turn_manager` | 14 个函数全是 HashMap 探测（`0x28` 步长）；无浮点常量 ⇒ 时限是数据驱动 |
| 目标选择 `target_manager` | SIMD 字节扫描，反编译不可读 |
| AI `function_evolutionator` | 跳转表 Ghidra 未建模；"批量试射"仍是推断 |
| 二进制功能 | 入口已可加载，但**未注册任何类**；且本机只有导出模板、无编辑器，未在真 Godot 里跑过（`docs/18` §6） |

## 5. 决策记录

- **上游报告不拆分**：`docs/09` 有判据（744 行 < 1200 阈值；新增内容仍属主线）。
- **`rsc/` 不入库**：二进制衍生素材 + 公开仓库涉及再分发；只提交 `MANIFEST.tsv`。
  用户若确认有权，再改 `.gitignore`。

## 6. 我犯过的错（别再犯）

1. 把**虚地址**当 Ghidra 地址传工具 → `not_found`（差 `0x100000`）。
2. 直接 grep 反编译文件的浮点常量 → 被文件头 `Ghidra 12.1.4` 的 `12.1` 污染。
3. 凭函数内部特征猜用途 → 把 `0x567530` 猜成"AI 选弹道"，查调用者后是**关卡**通视检查。
4. 用一次 `str.replace` 改文档却没核对结果 → README 索引**只更新了一半**（静默失败）。
5. 相信 `funcstarts.pkl` → 与 Ghidra 交集仅 2/13430。
6. 按 Godot 源码字面理解 `PACK_REL_FILEBASE` → 692 个文件 md5 全错，实测要用 `file_base`。
7. 把**文件偏移当虚地址**用 → 掩膜归属算错（`.text` 段 offset 与 vaddr 差 `0x1000`）。
   换算必须走 ELF 段表：`vaddr = sec.addr + (off - sec.offset)`。
8. 从局部伪代码推取整函数 → 把"截断"说成"四舍五入"；必须用输入输出对拍。

## 7. 下一步优先级

1. **让重建结果能构建成二进制**：`crate-type = ["cdylib"]` + 导出 `gdext_rust_init`；
   先用 `readelf -h` / `nm -D` 证明产物是 ELF 且带入口符号，再接真实 godot-rust。
2. 光栅化三条缺口（补齐即可实现 `rasterize`）。
3. 回合语义：先解 `0x28` 步长的那张表。

## 8. 文件地图

```
docs/00  状态总览          docs/08  坐标/掩膜      docs/14 目标选择
docs/01  架构              docs/09  报告拆分决策   docs/15 端到端（建议先读）
docs/02  模块地图          docs/10  生命死亡       docs/16 如何验证
docs/03  函数语言          docs/11  间接派发       docs/17 资源还原
docs/04  采点算法          docs/12  AI
docs/05  开火链路          docs/13  回合
```
