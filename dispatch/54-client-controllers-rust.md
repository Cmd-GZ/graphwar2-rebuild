# 54 · `agent/client-controllers-rust`：把 `client/src/screens/*.gd` 的**控制器逻辑**换回 Rust

**优先级 P0，但这是**最大的一块工程**。用户已明确放开禁区 `client/src/**`。**

## 0. 现状（先统一认识，别搞错边界）

```
client/src/views/*.gd      24 个 = **原版脚本逐字节副本**（第八轮 gdswap 换进来的）      ✅ 已是原版
client/src/screens/*.gd    17 个 / **6,055 行** = 我们照 .tscn 与真机真帧**重写的仿制品**  ❌ 要换掉
src/graphwar_*control*.rs  7 个 = **原版控制器的 Rust 复原**（play_screen / lobby / campaign /
                           player_screen / audio …）                                     ✅ 已经复原了
```

⇒ **问题不是"没有复原"，是"复原的 Rust 控制器没被客户端用起来"。**
GDScript 那边重复实现了一套状态机/流程，两份逻辑并存，谁也不知道哪份在跑。

## 1. 边界（这一条决定任务成败，务必按它做）

**Rust 负责"状态与流程"，GDScript 只负责"摆节点与画"。**

```
进 Rust：状态机、回合推进、按钮启用/禁用、事件分发、对局生命周期、胜负时机、报文装配
留 GDScript：节点层级、锚点/边距/字号（这些是视图属性）、纹理、动画
```

判据：**GDScript 里不许再出现"根据游戏状态决定做什么"的分支**（可以出现"根据状态改颜色/改文字"）。

## 2. 做法（**一轮只做一个屏**，别一次全换）

第一轮做 **`play_screen`**（最痛、也是唯一有真证据的）：

1. 读 `src/graphwar_play_screen_control.rs` 的现状，列出它已经覆盖的行为；
2. 把 `client/src/screens/play_screen.gd` 里**属于流程**的部分（约 6,055 行的多少？自己统计）改成
   调用 Rust（通过既有 GDExtension 类暴露的接口）；
3. 保持"视图属性"留在 GDScript；
4. `client/tools/play_screen_check.gd`（若无则新建）断言：
   ```
   流程行为现在由 Rust 侧驱动（可在 Rust 侧打断言/计数）
   GDScript 侧不再有流程分支（用 grep 证明：列出删掉的判断）
   画面上节点数量/位置与改前一致（headless 断言）
   ```

## 3. 硬约束

- **不许把 Rust 逻辑抄一份到 GDScript**（那是反向操作）；
- 数字不许退：`cargo test --offline` **1659**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；headless 全部脚本 0 失败；
  `selftest` **714 / 0 / 1 skip**（补 assets + serve_match 后）；
- ⚠️ gdext 构建三步（build → `cp` 进 `client/` → `--import`）；
- 提交信息用 `-F 文件`。

## 4. 环境

- 分支 `agent/client-controllers-rust`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- Godot `<WS>/tools/Godot_v4.7.2-stable_linux.x86_64`
- 证据：`docs/16-client.md`（控制器节）、`src/graphwar_play_screen_control.rs`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/client-controllers-rust`

## 5. 可以改的文件

- `client/src/screens/play_screen.gd`（**只删流程、留视图**）、`client/src/**`（相关）
- `src/graphwar_play_screen_control.rs`、`src/_scaffold/*_class.rs`（暴露接口）
- `client/tools/*.gd`、`docs/16-client.md`

❌ 不许改 `src/game.rs` 的结算、`fixtures/`、`dispatch/`

## 6. 验收

```text
① 6,055 行里属于"流程"的多少行（统计口径写清）
② 删掉了哪些流程分支（列出来）
③ Rust 侧新增/接上的接口（几个）
④ headless 与 selftest 数字（不许退）
⑤ GDScript 里还剩哪些"根据状态决定做什么"的分支（应为 0，或列出理由）
```

## 7. 汇报（≤10 行）

分支+sha、6,055 行的流程/视图拆分计数、删掉的分支、Rust 接口、
headless/selftest、剩余流程分支、`cargo` 结果。
