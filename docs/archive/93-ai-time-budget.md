> ⚠️ **运行时复核（`docs/100`）修正了本文一处过强的说法**：
> 「难度 6 实测跑不完」只对当时那台机器成立。同一次运行里出现过
> **13.4 s 跑满 1000 代 / 253000 次**，也出现过 25.007 s 截断在第 461 代。
> 正确表述：**25 s 预算存在、每代检查一次；难度 6 视机器与负载可能跑满也可能截断。**

# 93 · AI 进化的终止条件：代数上限 **+ 25 秒墙钟预算**

状态：**现行结论**。补上 `docs/AI.md` §8 里那条"是否有时间预算：**未知**"。

本文只做静态分析（REA/Ghidra + objdump），未改任何代码、未跑 `cargo test`。

---

## 0. 一句话结论

**有**时间预算，而且是**墙钟 25 秒**（`CLOCK_MONOTONIC`），不是帧数、不是 tick：

```
每一代开始前检查一次 —— elapsed > 25 s 就 break，把当前种群当结果返回。
```

终止条件因此是 **`代数到顶` ∨ `超过 25 秒`** 二者取先到。跳出去以后走的是**和"代数跑满"完全同一条出口**，
所以**调用方拿不到"我被截断了"这个信号**。

⇒ 难度 6（1000 代 × 250 种群）实测**跑不完**（§5），replay 里那一发很可能是被截断的中间种群。
难度 5 及以下（≤ 63250 次评分）实测都跑满。

---

## 1. 地址约定：先纠正一处会让人查错函数的混淆

`Ghidra 地址 = 文件 vaddr + 0x100000` 这条**是对的**（本次又独立验证了一遍），
但仓库里 `docs/12`、`archive/ai/74`、`docs/87` 把 **vaddr 写成了"Ghidra 地址"**。
拿这些数字去喂 `procedure_pseudo_code` 会查到一个**不相干的函数体中间**。

| 名字 | 文件 vaddr | Ghidra 地址 | 校验 |
|---|---|---|---|
| 进化入口（变体 A） | `0x51b440` | **`0x61b440`** | vaddr `0x51b440` 处字节 = `55 41 57 41 56 41 55 41 54 53 48 81 ec…`（函数序言）；Ghidra `0x51b440` 处字节 = `41 be 00 00 00 00`（`MOV R14D,0x0`，落在 `FUN_0051ae60` 内部） |
| 评分核心 | `0x51a750` | **`0x61a750`** | Ghidra `0x61b08f` = `MULSD XMM3,XMM0`（`f2 0f 59 d8`）= `archive/ai/74` 里写成 `0x51b08f` 的那条 |
| 子代工厂 | `0x4ee3a0` | **`0x5ee3a0`** | vaddr `0x4ee3a0` 是序言；`0x5ee3a0` 是 `94 24 58…`（垃圾） |
| token 分派器 | `0x4ef2b0` | `0x5ef2b0` | 按同一映射推出（未单独核该处字节；`0x4ef2b0` 本身是分派点、不是序言） |

同理 `archive/ai/74` 全篇的 `0x51b0xx` 都是 **vaddr**（字节对得上），对应 Ghidra `0x61b0xx`。
**本文一律用 vaddr 标注指令，Ghidra 地址另列一栏/加注。**

> 另一个副产品：`procedure_info` 在评分核心附近给出的函数边界是碎的
> （`FUN_00619950` 与 `FUN_0061a750` 的 body 交错），这是 Ghidra 把一个函数按冷热块拆开导致的，
> 不要据此判断"这里是两个函数"。真正的入口由 AI 入口里 5 个直接 `call 51a750` 决定；
> `analysis/funcstarts_ghidra_va.pkl` 在 `[0x51a750, 0x51b440)` 区间也只有 `0x51a750` 一个起点，
> 佐证整段（含 `0x51b0xx` 的适应度公式）属于同一个函数。

---

## 2. 进化主循环：在哪、怎么终止

### 2.1 伪代码骨架（Ghidra `procedure_pseudo_code 0x61b440`，删去分配/锁噪音）

```c
void FUN_0061b440(out, ctx, p3, p4/*key*/, p5/*代数*/, p6/*a5*/, p7/*每组个数=5*/, pop_vec)
{
  ...
  auStack_58 = FUN_009f3a10();            /* ← Instant::now()，只取一次，作为起点 */
  if (p5 != 0) {
    iStack_6c = 0;
    do {
      auVar36 = FUN_009f3a20(auStack_58); /* ← now - 起点 = Duration{secs, nanos} */
      if (auVar36._0_8_ == 0x19) {        /* secs == 25 */
        if (auVar36._8_4_ != 0) break;    /*   && nanos != 0  ⇒ 已超过 25 s ⇒ 跳出 */
      } else if (0x19 < auVar36._0_8_) break;   /* secs > 25 ⇒ 跳出 */
      ...这一代的全部工作（0.2 克隆 + 0.8 工厂子代 + 评分 + 排序 + 前 3 名 0.01 精评）...
      iStack_6c = iStack_6c + 1;
    } while (iStack_6c != p5);            /* ← 代数到顶 */
  }
  return pop_vec;                          /* 超时与跑满共用这个出口 */
}
```

### 2.2 逐条指令（变体 A = vaddr `0x51b440`，Ghidra `0x61b440`）

取起点：

```asm
51b459: mov  %r8d,0x118(%rsp)        ; 代数存到 rsp+0x118（r8d = 第 5 个参数）
51b71e: call *0x49f1d4(%rip)         ; # 9ba8f8 → Instant::now()
51b724: mov  %rax,0x160(%rsp)        ; 起点 tv_sec
51b72c: mov  %edx,0x168(%rsp)        ; 起点 tv_nsec
```

循环头（**唯一的每代时间检查**）：

```asm
51b7ae: lea  0x160(%rsp),%rdi        ; rdi = &起点
51b7b6: mov  0x49f82b(%rip),%rax     ; # 9bafe8 → elapsed(&起点)
51b7c4: xor  %ebx,%ebx               ; 代数计数器 = 0
51b7c6: mov  %ebx,0x14c(%rsp)        ; ←← 循环头
51b7cd: call *%rax                   ; elapsed() → rax = secs, edx = nanos
51b7cf: cmp  $0x19,%rax              ; ★ 25 秒
51b7d3: jne  51b7e0
51b7d5: test %edx,%edx               ;   nanos != 0 ?
51b7d7: je   51b7ea                  ;   nanos == 0 ⇒ 不算超过
51b7d9: jmp  51cc4d                  ; ★ 超时出口
51b7de: xchg %ax,%ax
51b7e0: cmp  $0x19,%rax              ; ★ 同一条 25
51b7e4: ja   51cc4d                  ; ★ secs > 25 ⇒ 超时出口
51b7ea: <这一代的循环体>
```

代数计数与回边：

```asm
51cb2f: inc  %ebx                    ; 这一代做完了
51cb49: cmp  0x118(%rsp),%ebx        ; 计数器 vs 代数（0x118 = r8d，2.2 第一行）
51cb58: lea  0x160(%rsp),%rdi        ; 回边重新装 &起点
51cb60: mov  0x49e481(%rip),%rax     ; # 9bafe8
51cb67: jne  51b7c6                  ; ←← 回边
51cb6d: jmp  51cbaf                  ; 代数跑满出口
...
51cc4d: mov  %r12,%rcx               ; ← 超时跳到这里
51cc50: mov  0xb8(%rsp),%rbp
51cc58: mov  0xc8(%rsp),%r14
51cc60: jmp  51cbaf                  ; 与"跑满"同一个出口
```

⇒ **两个出口在 `0x51cbaf` 汇合**，之后照常把当前种群写进返回值。
超时**不设标志、不改返回值语义**。

### 2.3 变体 B = vaddr `0x51d080`（Ghidra `0x61d080`）：同样的 25 秒

`FUN_0057a300` 按一个 `bool` 参数二选一：`param_15 == 0` 走 `0x51b440`，否则走 `0x51d080`
（两条 `call` 在 vaddr `0x47a41e` 与 `0x47a3d3`）。变体 B 结构一模一样：

```asm
51d096: mov  %r8d,0xdc(%rsp)         ; 代数
51d318: call *0x49d5da(%rip)         ; # 9ba8f8 → Instant::now()
51d31e: mov  %rax,0x208(%rsp)
51d326: mov  %edx,0x210(%rsp)
...
51d394: xor  %ebp,%ebp
51d396: call *%rcx                   ; ← 循环头；rcx = [9bafe8] = elapsed
51d398: cmp  $0x19,%rax              ; ★ 25 秒
51d39c: jne  51d3a7
51d39e: test %edx,%edx
51d3a0: je   51d3b1
51d3a2: jmp  51e543                  ; ★ 超时出口
51d3a7: cmp  $0x19,%rax
51d3ab: ja   51e543                  ; ★ 超时出口
51d3b1: <代循环体>
...
51de5d: inc  %ebp
51de5f: cmp  0xdc(%rsp),%ebp         ; 计数器 vs 代数
51de75: jne  51d396                  ; ← 回边
51de7b: jmp  51e543                  ; 跑满出口（就是超时那个目标）
```

⇒ 两个进化实现**都**被 25 秒兜底。整个 AI 模块（vaddr `0x51a750`–`0x51eb90`，4214 行反汇编）里
`cmp $0x19` 只有这 4 条（两对），**没有** 30 s / 60 s / 120 s 之类的第二套预算。

### 2.4 25 秒这个数从哪来：`clock_gettime(CLOCK_MONOTONIC)`

| Ghidra | vaddr | 事实 |
|---|---|---|
| `FUN_009f5bf0` | `0x8f5bf0` | **`clock_gettime(clockid, &ts)`** 再断言 `tv_nsec < 1e9`（Rust `Timespec::now`） |
| `FUN_009f3a10` | `0x8f3a10` | `return FUN_009f5bf0(1)` ⇒ `Instant::now()`，**`1` = `CLOCK_MONOTONIC`** |
| `FUN_009f3a20` | `0x8f3a20` | `now() - 起点` ⇒ `Duration{secs, nanos}`（借位逻辑在 `FUN_009f5b20`） |

- 两个函数指针来自 `.got` 槽的 `R_X86_64_RELATIVE` 重定位：
  `0x9ba8f8 → 0x8f3a10`（now）、`0x9bafe8 → 0x8f3a20`（elapsed）。
- **全 `.so` 里 `clock_gettime` 的 GOT 槽 `0x9bf6d0` 只有 1 处 rip 相对引用**：`0x8f5bff`
  （就在 `Timespec::now` 里）。⇒ 整个游戏只有这一个取时间入口，AI 的秒表就是它。
- `gettimeofday` 有 4 处调用（`0x6ef12d`、`0x6ef152`、`0x70bedd`、`0x70bf68`），**都不在 AI 路径上**。

比较的语义就是 Rust 的 `elapsed > Duration::from_secs(25)`（`(secs, nanos)` 字典序）：

| 情形 | 分支 | 结果 |
|---|---|---|
| `secs == 25 && nanos != 0` | `51b7d5 test` + `51b7d9 jmp` | **跳出** |
| `secs > 25` | `51b7e0`/`51b7e4 ja` | **跳出** |
| `secs == 25 && nanos == 0`（正好 25.000000000） | `51b7d7 je 51b7ea` | 不跳，再跑一代 |
| `secs < 25` | `51b7e4` 不跳 | 继续 |

> 严格说是 `> 25.000000000 s`。工程上等价于"每代开头看一眼表，到 25 就收"。

---

## 3. 预算的单位与粒度

| 项 | 值 | 说明 |
|---|---|---|
| 单位 | **秒**（`tv_sec`，秒级比较 + `tv_nsec` 只用来判"有没有零头"） | 不是帧、不是 tick、不是迭代次数 |
| 时钟 | `CLOCK_MONOTONIC`（`clockid = 1`） | 与墙钟时间无关，不受系统时间调整影响 |
| 起点 | 进化入口内、进入代循环之前 | 每**一次** AI 求值各自计时，不是全局共享 |
| 检查频率 | **每代顶部 1 次** | 所以最多超时**一代**（≈ 种群+3 次评分） |
| 常量位置 | 立即数 `0x19`（`cmp $0x19,%rax`），**不是**配置字段 | 六档难度共用 25 s，不随难度变 |
| 中断后行为 | 直接返回当前种群，与"跑满"同出口 | 无超时标志、无日志、无降级重跑 |

一次 AI 求值的评分次数（`archive/ai/67`：代数 ×（种群 + 3））：

| 难度 | 代数 | 种群 | 期望评分次数 | 距 25 s 安全吗 |
|---|---|---|---|---|
| 1 | 3 | 50 | 159 | 安全 |
| 2 | 10 | 100 | 1030 | 安全 |
| 3 | 25 | 150 | 3825 | 安全 |
| 4 | 100 | 200 | 20300 | 安全 |
| 5 | 250 | 250 | 63250 | 安全（实测跑满，见 §5） |
| **6** | **1000** | **250** | **253000** | **不安全（实测没跑满）** |

---

## 4. 单回合时限（`Timer30s/1m/…`）**不参与** AI 预算

结论：**不参与**。证据分三层。

1. **入口拿不到它。**
   `FUN_0061b440(out, ctx, ?, key, 代数, a5, 每组个数, 种群 Vec)` / `FUN_0061d080(…, 代数, a5, …)`
   ——实参里没有任何 tick / 毫秒 / 秒。上层的 `FUN_0053b3b0`（任务体，vaddr `0x43b3b0`）、
   `FUN_0057a300`（vaddr `0x47a300`）、`FUN_00538780`（vaddr `0x438780`）**三级包装里都没有取时间**：
   - 全 `.so` 只有 13 处读 `elapsed`（`0x9bafe8`），其中位于 AI 路径的只有 4 处
     （`0x51b7b6`/`0x51cb60` 在 `0x51b440`，`0x51d34d`/`0x51de66` 在 `0x51d080`），**全部是那两对循环头**；
   - `0x43b3b0`、`0x47a300`、`0x438780`、`0x51a750`（评分）、`0x4ee3a0`（子代工厂）**一处都没有**。
2. **AI 读的 ctx 字段里没有截止时刻。** 直接读的是 `ctx+0x10`（一个内部对象指针：其内
   `+0x14/+0x18/+0x20/+0x30/+0x38` 是锁标志、哈希表桶与掩膜）、`ctx+0x58`（复杂度上限 `limit`）、
   `ctx+0x5c`（函数模式 Normal/DiffEq）。没有 tick 类字段。
3. **回合时限是另一套、且是 tick 值。** `docs/42`：`end_of_turn_tick = 回合开始 tick + 时长`，
   `10800 = 180 s × 60`；`docs/35`：60 ticks/秒；`docs/13`：这类参数**不在代码里硬编码**（数据驱动）。

⇒ 回合时限（六档 `Timer30s/1m/2m/3m/5m/Inf`）只决定**服务端/状态机什么时候判本回合结束**，
不构成 AI 的计算预算；AI 的预算就是它自己写死的 25 s。

**顺带一个推断（未证实）**：25 s 恰好小于最短的非零回合时限 30 s。可能是刻意的
"AI 必须在人类倒计时用完之前算完"，但这只是数字巧合层面的推断，没有证据链。

**未测**：AI 运算跨过回合截止 tick 之后服务端会怎样（强制结束？）。`docs/35` §4 里
"服务端何时推进状态机"本来就仍列未知，本次没有推进它。

---

## 5. 对复现的影响（**这条会改结论**）

1. **难度 6 的样本大概不是 1000 代。**
   实测（`archive/ai/67`，frida 计数）：单次求值 `score` 计数 `217580`（`.rea-work/ai_opt6.json`）、
   `234278`（`.rea-work/ai_opt6_inf.json`），期望 `253000` ⇒ 分别少了 14.0% / 7.4%。
   同一份 `archive/ai/67` 记的用户录屏耗时是 **25.1 s / 21.4 s / 25.1 s**——两次正好卡在 25 s 上。
   ⇒ "少掉的那部分"与 25 s 上限吻合（`archive/ai/67` 当时把它归因于 hook 拖慢；两者并不互斥，
   但只要上限存在，**任何**机器都可能被截断）。
2. **难度 5 及以下实测跑满。** `.rea-work/ai_opt5.json`：两次求值各 `63250` 次评分
   （= 250 代 × 253），一次不少。
3. **截断点对机器敏感。** 25 s 能刷多少次评分取决于 CPU。按上面 217580 次/≤25 s 推，
   要跑完 253000 次需要约 29 s ⇒ **难度 6 就是临界点**，快机器也许刚好跑完。
   所以：**"难度 6 一定跑了 1000 代"和"难度 6 一定没跑完"都不能写死**；
   能写死的是——**replay 里难度 6 那一发不能用"1000 代跑满"当默认前提**。
4. **对复现实现的含义**：`src/aigen.rs` 的 `evolve` 若只按代数循环，复现难度 6 会**多算**；
   要等价就得加 `Instant` 上限 25 s，并且**接受结果依赖机器速度**（不可逐位复现）。
   难度 ≤5 不受影响。
5. **截断无痕**：超时和跑满共用出口，所以 replay / 网络包里**没有任何字段**能告诉你"这一发被截断了"。

---

## 6. 证据等级

按 `AGENTS.md` 的四档（逐位验证 = `cargo test` 覆盖；观测 = 能指到地址或字节；推断；未知）：

| 结论 | 等级 | 依据 |
|---|---|---|
| 每代顶部检查 `elapsed > 25 s` | **观测（逐条反汇编）** | `0x51b7cd` `0x51b7cf` `0x51b7d5` `0x51b7e0`/`0x51b7e4`；变体 B `0x51d396`–`0x51d3ab` |
| 秒表 = `clock_gettime(1)`（`CLOCK_MONOTONIC`） | **观测** | `0x8f5bf0` 内 `clock_gettime`；GOT 槽 `0x9bf6d0` 全库仅 1 处引用（`0x8f5bff`）；`FUN_009f3a10` 常量实参 `1` |
| 另一条终止条件是代数计数 | **观测** | `0x51b459`（存 r8d）、`0x51cb49`（`cmp`）、`0x51cb67`（`jne` 回边）；变体 B `0x51d096`/`0x51de5f`/`0x51de75` |
| 超时与跑满共用出口 | **观测** | `0x51cc60 jmp 51cbaf` 与 `0x51cb6d jmp 51cbaf`；变体 B 两处都跳 `0x51e543` |
| 两个变体都是 25 s | **观测** | `0x51d398`、`0x51d3a7` 实为 `48 83 f8 19`（与 `0x51b7cf` 同字节） |
| AI 模块内只有这一套时间常量 | **观测（有界否定）** | vaddr `0x51a750`–`0x51eb90` 全反汇编里 `cmp $0x19` 恰 4 条，无 30/60/120 s 比较 |
| 回合时限不参与 AI 预算 | **观测 + 推断** | 入口签名与三级包装无取时间（观测）；"所以只用于倒计时"（推断） |
| 难度 6 实测被截断 | **观测（运行时，他人实验）** | `archive/ai/67`；`.rea-work/ai_opt6*.json` 的 217580 / 234278 vs 253000 |
| 25 s < Timer30s 是刻意设计 | **推断（未证实）** | 只有数字关系，无代码关联 |

> 不能标"逐位验证"：按本仓约定那要 `cargo test` 覆盖，而本次任务明确禁止跑测试、也不改 `src/`。
> 若要升级：在 `src/aigen.rs` 给 `evolve` 加一个注入了假时钟的预算测试即可。

---

## 7. 还缺什么

1. **`FUN_0053b3b0` 是谁调的**：它没有已解析的调用者，只出现在数据槽 `0x955968`
   （`R_X86_64_RELATIVE → 0x43b3b0`），说明走函数指针/任务派发。
   ⇒ 如果**上游**还有"剩余回合时间不够就跳过 / 降档"的逻辑，本次未覆盖。
   但可以确定：`0x53b3b0` 本体及其往下**没有**任何时间判断。
2. **25 s 是不是"每回合一次"还是"每次求值一次"**：静态上起点在进化入口内、
   每次调用各取一次（观测）；但一个回合内会不会求值多次（例如重算）未追。
3. **难度 6 的真实完成代数**：依赖机器速度，静态给不出。要定案只能
   在**不 hook 评分函数**的前提下量一次单次求值的评分次数（hook `0x51b440` 入口/出口计时即可，
   这样不会像 `archive/ai/67` 那样把每次评分都拦下来）。
4. **`0x519450`/`0x519530`/`0x519690`** 三个函数各读一次 `Instant::now`（`docs` 未归属），
   已确认是 std 的 `Instant` 辅助（`checked_duration_since` 一类，含 `nanos == 1e9` 的哨兵分支），
   与 AI 无关；但它们的业务调用方未追。
5. **`TimerInf`（无时限）下为什么也被截断**：`archive/ai/67` 的 `ai_opt6_inf.json` 同样是 234278。
   与本文不矛盾（25 s 与回合时限无关），但可以作为独立旁证再抓一次。

---

## 附录 A · 复现（全部只读）

```bash
cd /home/user/MyProject/gw2
# 1) 循环头与 25 s 常量（vaddr 空间，objdump）
objdump -d --start-address=0x51b7a0 --stop-address=0x51b800 libgraphwarrust.so
objdump -d --start-address=0x51cb40 --stop-address=0x51cb80 libgraphwarrust.so
# 2) 变体 B
objdump -d --start-address=0x51d380 --stop-address=0x51d3c0 libgraphwarrust.so
# 3) 秒表实现
objdump -d --start-address=0x8f5be0 --stop-address=0x8f5c10 libgraphwarrust.so
# 4) GOT 槽指向谁（R_X86_64_RELATIVE 的加数）
readelf -rW libgraphwarrust.so | grep -E ' 8f3a10$| 8f3a20$|clock_gettime'
```

Ghidra/REA 侧（地址 = vaddr + 0x100000）：

```
procedure_pseudo_code 0x61b440      # 进化变体 A（含 25 s 检查）
procedure_pseudo_code 0x61d080      # 进化变体 B
procedure_pseudo_code 0x9f3a20      # elapsed()
procedure_pseudo_code 0x9f5bf0      # clock_gettime 包装
inspect_native_instruction 0x61b7cf # CMP RAX,0x19（48 83 f8 19）
```

临时产物放在 `.rea-work/`（本次新增 `ai_core.asm` 之外没有覆盖任何文件）。

## 附录 B · 本次用到的 REA 证据 id

| 证据 | 内容 |
|---|---|
| `ev_e0abcd88178d27b6af09ed3e59c42a17d286a4a0ca5cd08cda7e28ae802bcf25` | Ghidra `0x61b7cf` = `CMP RAX,0x19` |
| `ev_2984af7f2a0a3c81e019c46e2fdced01f121add7212fcab027911c73c65d39eb` | Ghidra `0x61b08f` = `MULSD XMM3,XMM0`（评分核心内的 vaddr `0x51b08f`） |
| `ev_c209b005711d83d0326ab99963d69a9d1582e654be0ea0cbb61e841a0190f3b3` | `FUN_009f5bf0` = `clock_gettime` 包装 |
| `ev_db677b96f1db6fccadb017857d662f12c6cb2dc701aa7dbae8087f2c54476fae` | `FUN_009f3a20` = `now - 起点` |
| `ev_545e1ee15ddc9279b1b0df7d77faff4cc3e9b8b1b0577eac1b714d14fc73e840` | `FUN_0061b440` 全函数伪代码（25 s 检查的上下文） |
