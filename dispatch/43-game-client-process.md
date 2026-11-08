# 43 · `agent/game-client-process`：定位 `GraphwarGameClient::process`（x86）—— 最后一环

**优先级 P0。第九轮报告 §8 第 1 条。** 它是"谁每帧驱动 `KernelBus::pump`"的最后一环；
接上之后两个输入端就完全走真链路。

## 0. 现状

```
容器：22 个 manager 全部注册（逐位）；11 wired / 2 layout / 7 assembled
输入端：pos_info_event 与 graphics 51/53/55 已换成真链路（第九轮 42 号）
缺：谁每帧调 pump  —— 原版是 `GraphwarGameClient::process`，**x86 侧还没定位**
```

现在是我们在 `client/` 里代驱动（`client/src/**` 是禁区，所以代驱动没法改成原版）。

## 1. 要做什么

1. **在 x86 目标里定位 `GraphwarGameClient::process`**：
   - 已知线索：arm64 有符号（第九轮报告 / `docs/02` 的模块表）；GDExtension 的 `process(delta)` 虚函数；
   - 手段（按可靠度）：arm64 符号 → 同源指纹（`tools/three_way_match.py` / `tools/callgraph_fingerprint.py` 已有）→
     **Windows 2.0.60 有 939 个真名**（同 ISA，查 `GraphwarGameClient` 相关最快，
     注意 COFF `Value` 是节内偏移，要加 `.text` 的 VirtualAddress）；
   - ⚠️ gdext 的引擎调用走 **GOT 槽**（`call *0x…(%rip)`），只看 `call rel32` 会漏。
2. **给出它的完整调用链**：`process` → 谁 → `KernelBus::pump`（或等价的"每帧派发"），逐环节地址；
3. **回答一件事**：`pump` 的**驱动频率**是引擎 `process(delta)` 还是自建 tick？（关系到我们代驱动的正确性）
4. 若定位成功：把"谁驱动"写清（我们仍不能改 `client/src/**`，但要**记录正确做法**并给出改造清单）。

## 2. 硬约束

- 结论要么指到指令、要么指到运行时观测，否则标 `推断/未知`；
- **负结论要有正对照**：说"扫不到"之前，先用已知槽（如 `0x9bac98`）证明扫法有效；
- 数字不许退：`cargo test --offline` **1631**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；headless `entity_drawer 236 / assemble 217 / plane_on_screen 96`。

## 3. 环境

- 克隆到 `<WS>/.agent-work/gcpp/repo`，分支 `agent/game-client-process`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/gcpp/target`
- x86 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`（Ghidra = +`0x100000`）
- arm64 `<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`
- Windows `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll`
- 工具 `tools/addr_probe.py`、`tools/loc_xrefs.py`、`tools/three_way_match.py`、`tools/callgraph_fingerprint.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/game-client-process`

## 4. 可以改的文件

- `tools/`（可扩展）
- `docs/02-module-map.md` / `docs/16-client.md` 追加一节
- `src/_scaffold/root_wiring.rs`（**只在**"驱动方式"需要记录时改注释）

❌ 不许改 `src/game.rs`、`client/src/**`、`dispatch/`

## 5. 验收

```text
① `GraphwarGameClient::process` 的 x86 地址（或"未定位 + 缺什么"）
② 到 `pump` 的完整调用链（逐环节地址）
③ 驱动方式：引擎 process(delta) / 自建 tick（结论 + 证据）
④ 若定位成功：我方需要改的清单（几处）
⑤ cargo test 1631；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、`process` 的 x86 地址（或未定位）、到 pump 的链路、驱动方式结论、
改造清单几处、`cargo`/`--check` 结果。