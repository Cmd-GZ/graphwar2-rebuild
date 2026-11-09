# 77 · `agent/manager-upgrade-2`：再升 4 格（Color / Soldier / Graphics / FHR）

**优先级 P0，纯静态活（`docs/161` 判这四格静态可解）。**

## 0. 现状

```
档位 **14 wired / 2 layout / 6 assembled / 0 stub = 22**
74 号已升：Rank / Name / LocalPlayer
本轮做剩下六格里**静态可解的四格**：
  Color       `+0x10` 字段语义
  Soldier     `add_soldier` 落点 ⇒ 它发不发 SoldierInfo(15)
  Graphics    `0xa8` 后 0x30 字节 + 第 3 处重播 tag
  FHR         **33 vs 35 口径冲突**（tsv 把 tag 35 叫 function_fire_event，而 docs/07 §7.2 说 33 才是）
```

**留着的两格别碰**：`Computer`（要 bot 局样本，见 79 号）、`GSM`（卡 `src/game.rs` 禁区）。

## 1. 每格三步（同 74 号口径）

```
① 字段布局逐字段解出（写入点 + 偏移 + 宽度）→ 到 `layout`
② 接事件流（订阅/发布都指到指令）+ 最小交互断言 → `wired`
③ 四处同步：`Wired::status()` 臂 / `*_TIER_COUNT` 常量（有编译期闸）/
   `client/tools/assemble_check.gd` 的 `TIER_TALLY` / `docs/161` 的清单
```

## 2. 特别要求

- **FHR 的 33/35 冲突必须先裁决**：读 `docs/07` §7.2 与 tsv 两边的依据，
  给出"哪个 tag 是 function_fire_event"，**裁决前不许照那个名字落码**；
- `Soldier` 那格要回答"它到底发不发 `SoldierInfo(15)`"——这直接影响 65 号的结算顺序；
- 证据一律 x86 逐条复核，**不许抄 arm64 结论**（74 号已经因此纠过一处）。

## 3. 硬约束

- 三轴不许退（`fire 647 / batch 647 / batch_conv 637`）；`cargo test --offline` **1710**；
  headless **1827 条 / 0 failed**；`selftest` 714（注意：它有 flake，见 76 号，失败先重跑一次再判）；
- **headless 只跑 `extends SceneTree` 的脚本**；
- 提交信息用 `-F 文件`。

## 4. 环境

- 分支 `agent/manager-upgrade-2`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- x86 目标 + Windows 2.0.60 真名（COFF `Value` 是节内偏移，要加 `.text` 的 `VirtualAddress`）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/manager-upgrade-2`

## 5. 可以改的文件

- `src/graphwar_{color,soldier,graphics}_manager.rs`、`src/graphwar_function_history_recorder.rs`
- `src/_scaffold/root_wiring.rs`、`client/tools/assemble_check.gd`
- `docs/161-manager-gaps.md`、`docs/16-client.md`
- ❌ 不许改 `src/game.rs`、`src/lib.rs`、`client/src/**`、`dispatch/`

## 6. 验收

```text
① 四格各自档位 before → after + 补的证据（地址级）
② 档位分布 14/2/6/0 → ?
③ FHR 33/35 的裁决（结论 + 依据）
④ Soldier 发不发 SoldierInfo(15)（结论 + 依据）
⑤ assemble_check 226 → ?；三轴/cargo/selftest 是否退
```

## 7. 汇报（≤8 行）

分支+sha、四格档位、FHR 裁决、Soldier 结论、档位分布、assemble_check 条数、三轴是否退。
