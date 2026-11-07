# 35 · `agent/residual-407`：把 34 号那套「(文本, 常数) 折叠复算」跑到 407 批上

**优先级 P0。第七轮报告 §6 第 1 条 —— 目前唯一「有机制、可复算」的入口。**

## 0. 现在的处境

```
fire/batch 两轴内容命中 647 / 1080 ⇒ 失败 433 批
其中 Y 189（爆炸点 y 末位差）＋ E1 127 ＋ X 91 = 407 批
```

已排除（都带数，别重推）：

```
① 求值器        eval_differ = 0（1287/1287）
② 点数组填充     真机：写进去的 y 与求值器返回逐位相同
③ pos_y 网络    已证伪
④ hole_point 回代 608 → 601，已回退
⑤ 常数挪 1..8 ulp 无收敛
⑥ 报文层        pos 与 end_x 读同一对 (ptr,len)、同一下标
⑦ 两条数组      34 号真机 37 发：两次 calculate_function 内容**逐位相同** ⇒ **否掉**
⑧ Y 桶 46 批 Normal：eval(回放文本, 回放 pos_x) 命中 0/46 ⇒ **回放自身不自洽**（在 Normal 上）
```

⇒ 现在**倾向**「回放自身不自洽」，但只在 **46 批 Normal** 上验过；
**DiffEq/Ode 那 114 发一发没覆盖**，统计上还不能下结论。

## 1. 要做什么

1. **把 34 号那套折叠复算做成通用工具**（输入：一批回放；输出：逐发的 `(文本, 常数, pos_x)` 与
   "用这些量复算出来的值 vs 回放值"）—— 34 号已有 `tools/` 里的原型与 `examples/` 探针，**复用不要重写**；
2. **在全部 407 批上跑一遍**，按两条轴分开统计：
   ```
   Normal 批（含之前的 46）：命中 / 不自洽 / 其它
   DiffEq(Ode) 批：命中 / 不自洽 / 其它      ← 这块是空白
   ```
3. **给「回放自身不自洽」下判决**：成立就写成结论（附逐批表），不成立就推翻并说明差在哪；
4. 若发现某类是**我方真错**（而不是回放不自洽），就地修并量化。

## 2. 判据要严

- **"不自洽"的定义**必须写死：例如"用回放自己给出的 `(文本, 常数, pos_x)` 复算，得不到回放的 `pos_y`"——
  这一条要**逐位**（`to_bits`），不是"差很小"；
- **正对照**：先用已确认能对上的批（内容命中那 647 里抽 20）验证你的复算链路**能得出"自洽"**，
  否则"全都不自洽"可能只是你的工具坏了；
- 所有数字带轴名与可复跑命令。

## 3. 环境

- 克隆到 `<WS>/.agent-work/res407/repo`，分支 `agent/residual-407`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/res407/target`
- 快照 `<WS>/.agent-work/replay-snapshot/`（401 份定格）
- 现成物：`tools/explpos_attrib.py`（39.3/39.6b 两张表）、`tools/posy_attrib.py`、
  `examples/posy_probe.rs`、`examples/nan_x0_probe.rs`、`tools/nan_c_probe.py`（unicorn 直调真函数）
- 护栏真值：`fixtures/guardrails.json`（fire/batch 647·1075、conv 637·1075、死亡 366/385、挖洞 207/294）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/residual-407`
- 逐文件 `git add`，**禁止 `git add -A`**

## 4. 可以改的文件

- `tools/`、`examples/`（只加不改）、`tests/`
- 若定位到我方真错：`src/game.rs` 或 `src/math_function.rs` 的**最小一段**（改前写清证据）
- `docs/06-verification.md` 追加一节

❌ 不许改 `*_class.rs`、`client/`、`dispatch/`

## 5. 验收

```text
① 工具（一条命令跑完 407 批），正对照 20 批能得出"自洽"
② Normal / DiffEq 两条统计（各：命中 / 不自洽 / 其它）
③ "回放自身不自洽" 的判决（成立/不成立 + 逐批表）
④ 有没有修我方实现；若修了，三轴 before/after（**一个都不许退**）
⑤ cargo test --offline 全绿（基线 1620）；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤10 行）

分支+sha、工具命令、正对照结果、Normal/DiffEq 两组统计、
判决（成立/不成立）、有没有改实现、三轴有没有退、`cargo`/`--check` 结果。