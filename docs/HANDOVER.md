# HANDOVER —— 给上下文压缩后的自己

**读这一篇就够了。** 它替代"翻 17 篇文档"的过程。若与本篇冲突，以代码与 `docs/` 原文为准。

最后更新：目标 2（"构建出二进制 + 还原资源"）刚起步时。

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
| 修正常数 + 端到端 | `src/shot.rs` | 端到端 + 上游 648 例 |
| 坐标变换 + 掩膜点测试 | `src/plane.rs` | 6 条 |
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
掩膜由 `play_screen_control` `0x422070` 分配（6 处 `600000` 常量），
`obstacle_manager` 填充与查询。

**资源**（实测）
PCK v4：索引在包尾（头部 `0x20` 处 `directory_offset`），
偏移基准是 `file_base`（**不是**包起点，flags=2 会骗人）。

**控制流**（观测）
`calculate_path` ← `shot_calculator`、`target_manager`；
`shot_calculator` ← `game_state_manager`、`function_evolutionator`（均经函数指针）。

## 4. 明确未解决（别假装做了）

| 块 | 卡点 |
|---|---|
| 光栅化 `0x4f11e0` | 已确认用 `f64::round`；缺 `dy` 循环端点/裁剪、第二个取整 `0xa30650`、"已占据不覆盖"分支 |
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
