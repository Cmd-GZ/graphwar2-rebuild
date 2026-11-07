# 33 · `agent/entity-drawer-draw`：补 `EntityDrawer::draw` —— **准备圈回到画面上**

**优先级 P0。这是用户能直接看见的回退，也是第六轮报告 §8 的第 1 条。**

## 0. 现状（31 号诚实登记的缺口，不是偷偷丢的）

```
client/src/screens/play_screen.gd:1284
  # ---- 准备圈：本轮**丢了**（诚实的缺口）
client/tools/selftest.gd:1849
  # 准备圈本身没有替身 ⇒ 画面上没有准备圈
docs/16 §48.5 第 2 条
```

根因：原版准备圈由 **`EntityDrawer::draw`（`0x4b2890`）** 画，
而我们的 `src/_scaffold/entity_drawer_class.rs` 的 `_draw` **一个 `draw_*` 都不调**
（`entity_drawer_class.rs:29` 的对照表里明确写着"没实现"）。
31 号的任务书禁止改其它 `*_class.rs` 的行为，所以只能删掉 GDScript 假圈 + 登记缺口。

## 1. 好消息：**算术已经复原好了**

```
src/entity_drawer.rs:246   pub fn ring_primitives(input: EntityRingInput) -> Vec<Primitive>
src/entity_drawer.rs:289   out.extend(ring_primitives(input));
```

准星几何（**3 次 `draw_arc` + 3 个外凸箭头**）是 `观测` 档，逐条立即数在 `docs/140` §2。
缺的只是「把 `Vec<Primitive>` 接到 Godot 的绘制调用上」。

## 2. 要做什么

1. 在 `entity_drawer_class.rs` 的 `_draw` 里**实现准备圈这一段**：
   取 `ring_primitives(...)` 的结果，逐条转成 `draw_arc` / `draw_polyline` / `draw_colored_polygon`
   （看 `Primitive` 有哪些变体，按 `docs/140` §2 的立即数定参数：半径、线宽、起止角、颜色）。
   ⚠️ 引擎调用要走 **GOT 槽**那套既有封装（`entity_drawer_class.rs` 里已有 `draw_arc` 的先例，见 `AGENTS.md`
   「间接调用与函数归属」第 1 条：`draw_arc` 包装 `0x8abca0` → 槽 `0x9bc020`）。
2. **别动 `src/entity_drawer.rs` 的算术**（那是已复原的部分）；`_scaffold` 只负责接引擎。
3. 在 `client/tools/entity_drawer_check.gd` 里**加断言**：
   ```
   准备圈存在：`_draw` 调了 ≥3 次 draw_arc（或 ring 原语条数 == 期望）
   角度/半径/线宽的数值与 docs/140 §2 的立即数一致（抽 2–3 条钉死）
   关掉准备态时**不画**（该有的分支判断）
   ```
4. 把 `play_screen.gd:1284` 那段"缺口"注释与 `selftest.gd` 的对应说明**改掉**（现在补上了）。

## 3. 验收（必须给数字）

```text
① entity_drawer_check：改前 72 条 → 改后 ?（新增的都是准备圈断言）
② 其余 headless 不许退：assemble 203 / plane_on_screen 96 / obstacle_drawer 116 /
   function_drawer 132 / entity_drawer(原 72)
③ selftest：改前 709 / 0 failed → 改后 ?（**要求 0 failed**）
④ 画面证据：截图或 viewport 抓帧（`docs/img/` 或说明用哪条断言代替；**不要提交二进制到仓库**）
⑤ cargo test --offline 全绿（基线 1585）；coverage --check OK；check_doc_refs rc=0
```

## 4. 环境

- 克隆到 `<WS>/.agent-work/ed-draw/repo`，分支 `agent/entity-drawer-draw`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/ed-draw/target`
- Godot `<WS>/tools/Godot_v4.7.2-stable_linux.x86_64`
- ⚠️ **gdext 构建三步缺一不可**：`cargo build --release --features gdext-bindings`
  → **`cp` 进 `client/`** → `godot --headless --path client --import`
- 证据来源：`docs/140` §2（立即数逐条）、`src/entity_drawer.rs` 的 `ring_primitives`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/entity-drawer-draw`
- 逐文件 `git add`，**禁止 `git add -A`**

## 5. 可以改的文件

- `src/_scaffold/entity_drawer_class.rs`（**本任务就是要改它的 `_draw`**）
- `client/tools/entity_drawer_check.gd`（加断言）
- `client/src/screens/play_screen.gd`（只改那段缺口注释）、`client/tools/selftest.gd`（同理）
- `docs/16-client.md`（§48.5 第 2 条就地更新）

❌ 不许改 `src/entity_drawer.rs` 的算术、`src/game.rs`、`client/src/**` 的其它逻辑

## 6. 汇报（≤8 行）

分支+sha、`_draw` 里画了几条原语（分别是什么）、entity_drawer_check 条数 before→after、
其余 headless 有没有退、selftest 数、`cargo`/`--check` 结果。