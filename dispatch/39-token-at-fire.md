# 39 · `agent/token-at-fire`：抓**开火那一刻**真正喂进 `calculate_function` 的 token 列表

**优先级 P0。第八轮报告 §8 第 1 条 —— 这是 407 批「回放自身不自洽」最后一块无法排除的替代解释。**

## 0. 为什么非抓它不可

第八轮证明：用回放**自己**的 `(corrected_function, pos_x)` 复算，得不到它**自己**的 `pos_y`，407/407 覆盖。
但那一条有个前提：**回放里记的文本就是当时真正喂进采样器的文本**。

```
回放里只有两种表示：① 函数原文  ② 加上「± 3 位小数」修正量的串
如果真机当时喂进去的是**第三种**（例如另外拼了括号/前后缀/空格的变体），
那么"回放不自洽"的结论就要打折扣 —— 因为它记错了输入
```

⇒ **抓一次真的**，这一步做完，407 那条结论才算钉死。

## 1. 要做什么

在真游戏里打一炮，在 **`calculate_function`（x86 `0x3c2530`）入口**断下，把**实参**抓下来：

```
① 它吃的**文本串**是什么（`&str` / `String` 的 ptr+len）
② 与之对照：回放里记的 `corrected_function`、`function` 两种表示，**逐字节**比较
③ 若是第三种形态，**把它记下来**（这是新事实，直接改我们对"修正文本"的模型）
④ 顺手记：`x0/y0` 的 bits（与 `start_x/start_y` 对比）
```

**判据是逐字节**（`memcmp`/hex 相等），不是"看起来一样"。

## 2. 复用现成装置（别重造）

- `tools/frida_point_array.{js,py}` —— 已经在 `0x9bac98` 槽与 `calculate_function` 上打过断点，
  34 号就是用它抓 `(ptr,n)` 的；
- `docs/06` §35.1 那三个坑（`/tmp` 逐命令 tmpfs ⇒ Xvfb 用 TCP；EGL 不必设 mesa；Vulkan→GL3）；
- 前一发的样本格式见 `fixtures/frida_point_array_sample.jsonl`（provenance + 每次开火一行）。

## 3. 环境

```
游戏三件套   <WS>/.reference/game/{Graphwar.x86_64, Graphwar.pck, libgraphwarrust.so}
             sha256 9a538115… ＝ 逆向目标，逐字节相同
frida        <WS>/.venv 里（本机验证过 17.26.1 可用）
Xvfb         /usr/bin/Xvfb（TCP 监听绕开 /tmp 逐命令 tmpfs）
```

- 克隆到 `<WS>/.agent-work/tokfire/repo`，分支 `agent/token-at-fire`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/tokfire/target`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/token-at-fire`

## 4. 纪律

- **样本必须入库**（文本 JSON，去掉路径/用户名）；
- **地址必须带构建**（sha256 `9a538115…`）；
- 抓不到就**如实报"做不了 + 卡在哪"**，不许用静态推测代替运行时结论。

## 5. 可以改的文件

- `tools/frida_point_array.*`（扩展；改了就说明为什么）
- `fixtures/`（新增样本）
- `docs/06-verification.md`（追加一节，标 `观测`）

❌ 不许改 `src/`、`client/`、`dispatch/`

## 6. 验收

```text
① 抓到几次开火的文本（几次、几发）
② 与回放的两种表示逐字节比较的结果（相同/不同，不同在哪）
③ 若有第三种形态：它是什么、怎么拼出来的
④ 对 407 批的意义：那条「回放自身不自洽」的结论是**加强**还是**需要修正**
⑤ cargo test --offline 全绿（基线 1628）；check_doc_refs rc=0
```

## 7. 汇报（≤8 行）

分支+sha、抓到几次、逐字节比较结果、第三种形态（有/无 + 是什么）、
对 407 批结论的影响、样本文件、`cargo` 结果。