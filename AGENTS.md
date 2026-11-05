# AGENTS.md —— gw2-rebuild 仓库约定

## 0. 这个项目的标准（**先读这一节，它决定你怎么评价自己的工作**）

**最终目标：还原出这款游戏的源代码。**
不是"造一个能玩的替代品"，也不是"造一个第三方 open version"。
**构建出一个行为一致的二进制只是第一步**；源码级还原（文件 / 模块 / 函数边界 / 数据结构能对上）才是终点。
使用者拿这些源码去**分析游戏行为、写攻略**——所以"人能读懂的解释"和代码同等重要。

由此推出六条硬标准：

1. **没有什么是"不重要"的。** 原版 `libgraphwarrust.so` 有 **13503 个函数、57 个
   `graphwar_*.rs` 模块**，全部在范围内。准备圈、弹窗高度、图标底色**都是源码**——
   它们分别属于 `entity_drawer` / `play_screen_control` 等模块，不是"可以不管的外观细节"。
2. **进度按原版的分母算，不按我们自己的测试条数算。**
   `cargo test` / `selftest` 的条数**是回归保护，不是复原证据**。
   复原进度 = 原版可归属函数里，我们**还原并验证**了多少。
3. **每条结论必须标证据等级**：`逐位`（与真二进制逐字节/逐位对拍）/ `观测`（抓包、真机、运行时）/
   `推断`（有依据但未证）/ `未知`。**绝对不许把"推断"写成陈述句。**
   做不到的就说做不到——这个项目里"诚实标未知"比"看起来做完了"值钱得多。
4. **客户端也是源码，但现在的 `client/` 不是原版源码。**
   原版的界面控制器在 **Rust** 里（`play_screen_control`、`entity_drawer`、`player_screen_control` …）；
   包里那 105 个 `.gdc` **全是小控件 + AdMob 插件**，没有 `play_screen` / `lan_lobby` 这些控制器。
   ⇒ 现在 `client/src/screens/*.gd` 是**照 `.tscn` 与截图重写的仿制品**，
   它的**流程/时序**（回合推进、准备态、弹道生命周期、胜负时机）**基本没有证据**，已实测出多处与原版不符。
   正确方向：**控制器迁回 Rust（按原模块名）+ 视图层改用反编译出的原版 `.gd`**。
5. **每一屏/每个行为，要么指到 `反汇编地址`，要么指到 `真实报文/真机真帧`，否则不许写"已复原"。**
   写测试的人不许同时是"判定测试通过标准的人"——自写断言只能证明自洽。
6. **文档不许虚报。** 发现旧文档"过老"或"虚报"（例如把已做完的写成没做、或把测试条数当进度），
   **就地改正并说明改了什么**，不要留着让后来的人误判。

## 三条硬规矩

1. **不把推断写成结论。** 每个结论必须能标成四档之一：
   **逐位验证**（与**真二进制**逐字节/逐位对拍，**不是**"cargo test 过了"）/
   **观测**（能指到反汇编地址、真实报文或真机真帧）/ **推断**（写明缺什么证据）/ **未知**。
   文档里出现"应该是""大概是"这类措辞时，必须同时写清缺什么证据。
   ⚠️ 与 §0 第 2 条一致：**`cargo test` / `selftest` 的通过只是回归保护，不构成复原证据。**
2. **代码不自我证明。** 行为正确性只由真二进制决定。改动 `src/` 后：
   `cargo test` 必须全绿；若夹具要改，说明真值变了，必须回到上游 `analysis/` 用
   unicorn 对拍复核（见 `docs/16-reproduce.md` §4）。
3. **不提交二进制衍生内容。** `rsc/`（游戏资源）、`*.pck`、反编译产物一律 gitignore。
   上游 `AGENTS.md` 有同样的规则，理由也适用于此处（体积 + 版权）。

## 目录

```
src/          Rust 复原实现（内核 + 对拍/回放等新模块）
tests/        与真二进制对齐的验收（11 个 test 二进制；条数会变，别写死在这里）
fixtures/     真值夹具（已入库；重新生成见 tools/gen_fixtures.py）
examples/     可跑的小工具（serve_match / replay_diff / diff_conv / ai_explain …）
tools/        分析脚本（callers/ptr_slots/const_xrefs/pck_extract/module_map/propagate/
              campaign_data/campaign_params/event_tags/replay_state/gdc_* …）
client/       Godot 客户端 ⚠️ **不是原版源码**，见 §0 第 4 条
docs/         观测/推断/决策/操作说明 —— **索引在 docs/INDEX.md**，不在 README
docs/guide/   （规划中）攻略向行为手册
recovered-src/（规划中）从 pck 回收的原版文本源码：.gd/.tscn/.tres/project.godot
rsc/          ← gitignore，用 tools/pck_extract.py 生成
.agent-work/  ← gitignore，子 agent 隔离工作区与证据
```

⚠️ **这里的数字只写"结构"，不写"条数"**：条数每轮都在变，写进来必然过期（历史上写过"21 条""17 篇"，都过期了）。

## 常用命令

```bash
export CARGO_HOME=$PWD/../.cargo-home CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo test --offline                      # 全绿即可，别把条数当进度（见 §0.2）

../.venv/bin/python tools/callers.py --module graphwar_turn_manager.rs
../.venv/bin/python tools/ptr_slots.py 0x3c2bc0      # 函数指针派发
../.venv/bin/python tools/const_xrefs.py 50.0 30.0   # 常量反查
../.venv/bin/python tools/pck_extract.py --verify    # 还原 rsc/
```

## 反复踩过的坑（"改了但没生效"这一类的必须一次记全）

- **地址空间**：反编译里的 `_DAT_xxxx` 是 **Ghidra 地址**，读原始字节要减 `0x100000`；
  但 `Caller`/`Callee` 类工具返回的也是 Ghidra 地址，别拿虚地址去查。
- **调用图会骗人**：Rust 大量用函数指针派发，直接调用为零**不代表**没被使用。
  写"未观测到调用"，并用 `tools/ptr_slots.py` 补查。

## 提交

不要直接 `git push`（本机 ssh 配置有权限问题）：

```bash
GIT_SSH_COMMAND="ssh -F /dev/null" git push origin main
```

上游 `Cmd-GZ/graphwar2-analysis`（= `../`）与本仓库互不包含；本仓库的内容**绝不**提交到上游。

两个仓库都在 **2026-11 改过名**（旧名 `graphwar-algorithm-analysis` / `graphwar2` 已失效）：
`git@github.com:Cmd-GZ/graphwar2-analysis.git` 与 `git@github.com:Cmd-GZ/graphwar2-rebuild.git`。

子 agent 一律推**自己的分支**（`git push origin HEAD:refs/heads/agent/<topic>`），不要直接推 `main`。

## 间接调用与函数归属（2026-11 新学到的，务必照用）

1. **不要只看 `call rel32` 判断"有没有调用这个引擎 API"。**
   gdext 的引擎函数指针走 **GOT 槽**（`.rela.dyn` 的 `R_X86_64_RELATIVE` 写入），
   调用点是 `call *0x??????(%rip)`。只看 `call rel32` 会得出"原版不画任何东西"这种荒谬结论。
   实例：`draw_arc` 字面量 `0x19f9d0` → 包装 `0x8abca0` → GOT 槽 `0x9bc020` →
   全 `.text` 仅 5 条调用，其中 3 条在 `EntityDrawer::draw`（vaddr **`0x4b2890`**）。

2. **函数 → 源文件归属用 Rust 的 `panic::Location`。**
   它在 `.data.rel.ro`，里面的指针是**重定位**（搜裸指针一无所获）：
   要解析 `.rela.dyn` 把"文件名字面量地址 + 行号"一起还原。
   自校验方式：`0x4eb9d0`（求值器）/ `0x4ed670`（解析器）应落 `src/math_function.rs`。

3. **"改了但没生效"的三个静默陷阱**（都真踩过，且都不报错）：
   - 新增 `.gd` / `.gdextension` 后没有 `godot --headless --path client --import` ⇒ Godot 静默不加载；
   - `cargo build` 后没覆盖 `client/libgw2_rebuild.so` ⇒ 客户端跑**旧内核**（`dump` 里会少方法）；
   - 仓库根残留旧的 `target/debug/examples/serve_match` ⇒ 自检连上的是**旧服务端**。
     现在 `selftest` 会优先读 `CARGO_TARGET_DIR`。

4. **`/tmp` 是逐命令 tmpfs**，跨命令的文件会凭空消失；临时产物一律放 `.agent-work/`。
