# 74 · `agent/manager-upgrade`：9 个 `assembled` manager 升档（按 `docs/161` 的顺序做 3 个）

**优先级 P0，纯静态活（7 格静态可解，不需要开游戏）。**

## 0. 依据：`docs/161-manager-gaps.md`（先读它，别重推）

```
档位现值：**11 wired / 2 layout / 9 assembled / 0 stub = 22**（三处口径一致：`Wired::status()` 臂 /
四个 `*_TIER_COUNT` + 编译期闸 / `assemble_check.gd` 的 `TIER_TALLY`）
九格各缺什么、哪格静态可解，全在 docs/161 的清单里
```

## 1. 本轮只做 **3 个**（按它给的性价比顺序）

| # | manager | 它说缺什么 | 备注 |
|---|---|---|---|
| 1 | **`GraphwarRankManager`** | `+0x08` 是否进序列化 | 最便宜（2 订阅 + 2 发布 + 1 表） |
| 2 | **`GraphwarNameManager`** | `+0x38`→`+0x48` 8 字节 + 基址口径 `0x50/0x58` | `new` 已钉 **`0x519100`**（tsv 里 4 个候选全错，`0x4500c0` 是 `game_server_thread`） |
| 3 | **`GraphwarLocalPlayerManager`** | `+0x20` 槽 + 5 条请求的门 | 影响面最大（50/52/54/59/66 五条请求的唯一发起侧）；`new` 内联在 `GraphwarGameClient::new`(`0x517690`)，3 条订阅在 `0x5179d4/0x517a27/0x517a77` |

**每格三步**（与 40 号同口径）：① 字段布局逐字段解出（写入点 + 偏移 + 宽度）→ `layout`；
② 接事件流（订阅/发布都要指到指令）→ 加交互断言后 `wired`；
③ 在 `client/tools/assemble_check.gd` 加断言，并把 `TIER_TALLY` 与 `*_TIER_COUNT` 同步改。

⚠️ **GSM（`GraphwarGameStateManager`）别碰**：它不是缺证据，是卡 `src/game.rs` 禁区。

## 2. 硬约束

- **不许改 `src/game.rs`**（禁区）；需要它的证据就登记"卡禁区"，别硬来；
- 档位必须**四处同步**：`Wired::status()` 臂 / `*_TIER_COUNT` 常量（有编译期闸）/
  `assemble_check.gd` 的 `TIER_TALLY` / `docs/161` 的清单；
- 数字不许退：`cargo test --offline` **1694**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；headless **14 个 SceneTree 脚本 / 1818 条 / 0 failed**；
  `selftest` **714 / 0 / 1 skip**；
- **headless 只跑 `extends SceneTree` 的脚本**（`*_check.gd`）；拿 `plane_on_screen.gd` /
  `screenshots.gd` / `selftest.gd` 喂 `--script` 会弹 ALERT + 留僵尸实例污染后续；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/manager-upgrade`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`（Ghidra 地址 = +`0x100000`）
- Windows 2.0.60 有 **939 个真名**可查（COFF `Value` 是节内偏移，要加 `.text` 的 `VirtualAddress`）
- 工具：`tools/addr_probe.py`、`tools/rela_slot.py`（69 号新增）、`tools/loc_xrefs.py`、`tools/rust_sym.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/manager-upgrade`

## 4. 可以改的文件

- `src/graphwar_{rank,name,local_player}_manager.rs`、`src/_scaffold/root_wiring.rs`
- `client/tools/assemble_check.gd`（断言 + `TIER_TALLY`）
- `docs/161-manager-gaps.md`（把已完成的三格标上）、`docs/16-client.md`（追加一节）
- ❌ 不许改 `src/game.rs`、`src/lib.rs`、`client/src/**`、`dispatch/`

## 5. 验收

```text
① 三格各自的档位 before → after，以及"缺什么"被怎么补上的（地址级）
② 档位分布：11/2/9/0 → ?
③ assemble_check 条数 before 217 → ?（且 0 failed）
④ 四处档位同步（贴证据：status() 臂 / 常量 / TIER_TALLY / docs/161）
⑤ 三轴与 cargo/selftest 有没有退
```

## 6. 汇报（≤8 行）

分支+sha、三格档位变化、补的证据（地址）、档位分布、assemble_check 条数、
三轴/cargo/selftest 是否退。
