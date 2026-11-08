# 66 · `agent/token-triple`：**三样同时抓**，判"修正常数 token 差 1 ulp"

**优先级 P0。这是 60 号留下的**唯一活路**（它已把其他路全部堵死）。**

## 0. 现状（60 号的结论，别重推）

```
`pos_y` 全 .text 只有两条写入指令，**一条 16 字节 `movdqu` 同时写 x 和 y**
（`0x4729a0` / `0x472e14`），源分别是队列点数组 `pts[n-1]` 与点位表
⇒ **结构上不可能只动 y**
unicorn 复现不出回放那个值
唯一能产出它的单 ulp 旋钮 = **修正常数 token「8.046」+1 ulp**（`y=e·c+8.046`，y 动 1、x 不动）
⇒ 与观测形状吻合，但只是**推断**
```

## 1. 要做什么：**同一发上同时抓三样**

选一个**已知差 1 ulp 的分叉批**（60 号报告里有清单；也可从新样本里取），在真游戏里重放/重打：

```
① `calculate_function`（`0x3c2530`）**入口**：token 数组（`&[Token]` 的 ptr + len）与
   **修正常数那个 token 的 16 字节原样**（tag u64 + value f64）
② `0x472865` 附近：`(rdx, rcx)` + 那次读到的 **16 字节**
③ 同一发最终那条 `EffectInfo` 的 JSON（pos_x / pos_y）
```

**判据**：把 ① 里的常数 token 与"回放文本 parse 出来的常数"逐字节比：
```
· 若①的常数与"回放文本 +1 ulp"完全一致 ⇒ **假设成立**（打印/回代确实丢了 1 ulp）
· 若①的常数与回放文本**完全一致** ⇒ 假设否，1 ulp 在更下游
```

## 2. 纪律

- **三样必须同一次抓**（分开抓就对不上了，这是本任务的核心要求）；
- 样本入库（文本 JSON，去路径用户名）；
- 抓不到就如实报"卡在哪 + 缺什么"；
- 提交信息用 `-F 文件`。

## 3. 环境

```
<WS>/.reference/game/  三件套（sha 9a538115…）
frida 17.26.1          <WS>/.venv
Xvfb（TCP 监听）        docs/06 §35.1 的三个坑
样本                   新录的 5 份在 <WS>/.agent-work/replay-new/
```

- 分支 `agent/token-triple`
- **私有** `CARGO_TARGET_DIR`
- 现成：`tools/frida_point_array.*`（已在 `0x9bac98` 槽与 `calculate_function` 上打过断点）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/token-triple`

## 4. 可以改的文件

- `tools/frida_point_array.*`、`examples/`、`fixtures/`（样本）
- `docs/06-verification.md` 追加一节（标 `观测`）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 抓到几发、三样是否同一次
② 常数 token 的 16 字节 vs 回放文本 parse 出的值（逐字节）
③ 那次 16 字节 load 的值 vs `EffectInfo.pos_y`
④ 判决：常数 +1 ulp 假设成立 / 否 / 无法判定
⑤ cargo test 1671；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、抓到几发、常数 token 对比结果、load 值 vs pos_y、判决、`cargo` 结果。
