# 97 · 两阶段评分：全部粗评 0.5 → 前 3 名精评 0.01 → "不一致"就整条换新

状态：结论性（静态，反汇编逐条）。回答"`docs/95` §2.7 那一句到底怎么判"，
并把规则落进 `src/graphwar_function_evolutionator.rs`。**本轮没有新的运行时实验**，运行时数据一律转引
`docs/archive/ai/67`、`docs/archive/ai/74`、`docs/archive/ai/75` 并单独标注。

地址约定同全仓：**Ghidra 地址 = ELF vaddr + 0x100000**。本文全部写 **vaddr**
（objdump 的输出就是 vaddr），需要 Ghidra 地址时自己加 `0x100000`。
本文引用的驱动函数 = vaddr `0x51b440`（Ghidra `FUN_0061b440`），
评分核心 = vaddr `0x51a750`（Ghidra `FUN_0061a750`，⚠️ 旧文档 `docs/12`/`74` 曾把它
写成 Ghidra `0x51a750`，那是 vaddr）。

---

## 0. 结论（一页）

```
每代：
  ① 组新一代：s 个槽 × per_slot(=5) 个个体 = 5s = 种群
     每槽 = 1 个克隆 + 4 个工厂子代
  ② **每一个个体**（含克隆）都在 step = 0.5 上**粗评**一次
  ③ 按分数**降序排序**，**截断**只留前 s 个（s = 种群/5 = 20%）
  ④ 前 3 名用 step = 0.01 **精评**（三名各一次，三段展开的代码）
  ⑤ 精评与粗评"不一致" ⇒ 该名次**整条换成**"随机函数 → 补齐 → 变异"
```

**"不一致"的确切判据**（不是排序变了，也不是"分数差超过阈值"）：

```
不一致 ⟺ (hits_fine ≠ hits_coarse)  ∨  (|dist_fine − dist_coarse| > 0.5)
```

其中 `hits` = 评分结果里的**命中数项**（适应度里乘 2e6 的那个整数），
`dist` = 评分结果里被减掉的**距离项**（未命中时是 1e6 的哨兵）。
**不是** token 长度，也**不是**适应度总分。

其余四条答案：

| 问题 | 答案 |
|---|---|
| 精评分数是否写回？ | **不写回**。一致时什么都不做；不一致时也只覆写 token 向量，分数字段保留旧粗评值 |
| 换掉谁 / 新的从哪来 | 换掉**该名次那一条**（第 0/1/2 名）；新的 = `makeRandomFunction → 构造/补齐 → 变异`，与槽循环的"随机分支"是**同一套调用、同样的 RNG 消耗** |
| 前 3 还是前 N | **硬编码 3**，三段展开、无循环常量、无配置字段（三名各带 `len > rank` 守卫） |
| 两阶段是否共用同一公式 | **是**。同一个评分核心 `0x51a750`、同一个上下文、同一套"修正后加 `+ c`"的 token 预处理，**只有 `xmm0`（步长）不同** |
| 粗评是否只评新生成的 | **全部评**，克隆也重新评（克隆只复制 token，分数是新算的）⇒ 每代 `种群 + 3` 次评分 |

---

## 1. 地址总表（vaddr）

| # | 事项 | 地址 |
|---|---|---|
| 1 | 驱动函数（外循环 = 代数） | `0x51b440`；代数递增 `0x51cb2f`，比较 `0x51cb49` |
| 2 | 每代顶部的 25 s 墙钟检查 | `0x51b7cf cmp $0x19`（`docs/93`） |
| 3 | 槽循环 | 头 `0x51b829 cmp %r14,%r12` / `0x51b82c jbe 0x51b900`；尾 `0x51bda1 inc` `0x51bda7 jne 0x51b829` |
| 4 | 克隆 = 深拷贝 token | `0x51b8c5 call *0x9ba638`（memcpy）→ `0x51b8fa jmp 0x51b961` |
| 5 | 槽的"随机分支" | `0x51b90f call 0x4ef020`（makeRandomFunction）、`0x51b923 call 0x4ee0e0`（构造/补齐）、`0x51b95c call 0x4ee720`（变异） |
| 6 | token 向量复制（评分前） | `0x51b993 call 0x4ebbe0` |
| 7 | 修正的门（`ctx+0x5c`） | `0x51bbaa cmpb $0x0,0x5c(%rsi)` / `0x51bbae jne 0x51bc7a` |
| 8 | 修正用的求值器 | `0x51bbde call 0x4eb9d0`（`c = y0 − f(±x0)`：`0x51bc43`–`0x51bc4c`） |
| 9 | 把 `f` 变成 `(+) f c` | 前插 `+`：`0x51bc21 movq $0x3`；尾部追加 `const c`：`0x51bc5e movq $0x11` |
| 10 | **粗评（克隆那一份）** | `0x51bc8b movq 0x19f6d0`(=0.5) → `0x51bc93 call 0x51a750` |
| 11 | 粗评结果写进个体 | `0x51bcc7`–`0x51bcfc` |
| 12 | 工厂子代循环 | 体 `0x51bdb2`；工厂 `0x51be93 call *0x9bc288`→`0x5ee3a0`；`0x51c19c dec` / `0x51c19e jne 0x51bdb2` |
| 13 | **粗评（工厂子代那一份）** | `0x51c0c1 movq 0x19f6d0` → `0x51c0c9 call 0x51a750`；写回 `0x51c105`–`0x51c132` |
| 14 | 代末排序分派 | `0x51c1e2 cmp $0x2` / `0x51cb6f cmp $0x15` / `0x51cb7f call 0x41b500` / `0x51cb96 call *0x9bbdc0`(→`0x4605f0`) |
| 15 | 截断到 s | `0x51c1f0`–`0x51c210`（`0x51c203 sub %r15,%rbx`、`0x51c208 mov %r15,0xb0(%rsp)`） |
| 16 | 一致性基准（第 0/1/2 名） | `0x51c2b5`/`0x51c2be`、`0x51c5f0`/`0x51c5fd`、`0x51c8a7`/`0x51c8b7` |
| 17 | **精评 #0** | `0x51c3ab movsd 0x19f110`(=0.01) → `0x51c3b3 call 0x51a750` |
| 18 | **精评 #1 / #2** | `0x51c6e6`/`0x51c6ee`；`0x51c9a0`/`0x51c9a8` |
| 19 | **判据** | `0x51c3b8`–`0x51c3e4`（`#1`：`0x51c6fa`–`0x51c71f`；`#2`：`0x51c9b4`–`0x51c9d9`） |
| 20 | **换新 #0** | `0x51c3ea`；`0x51c3f6 call 0x4ef020`、`0x51c40a call 0x4ee0e0`、`0x51c44c call 0x4ee720`；写回 `0x51c480`–`0x51c495` |
| 21 | **换新 #1 / #2** | `0x51c725`–`0x51c787`（写回 `0x51c7bc`）；`0x51c9df`–`0x51ca41`（写回 `0x51ca76`） |
| 22 | 评分核心的 sret 写回 | `0x51b10d`(score,+0x00)、`0x51b11d`(dist,+0x08)、`0x51b111`(hits,+0x10)、`0x51b114`(+0x14) |

---

## 2. 两套 48 字节布局（判据为什么是这两个字段）

**评分结果**（`call 0x51a750` 的 sret，24 字节，`0x51b10d`–`0x51b11d`）：

| 偏移 | 内容 | 写入点 |
|---|---|---|
| `+0x00` f64 | `score`（排序键）= `hits*2e6 − dist − V²/100 − (V>limit ? 5e7 : 0)` | `0x51b10d` |
| `+0x08` f64 | `dist`（距离项，初值 1e6、按事件取 min、被**减**掉） | `0x51b11d` |
| `+0x10` u32 | `hits`（命中数项 = 乘 2e6 的那个整数） | `0x51b111` |
| `+0x14` u32 | 未定（同一路径里由 `esi` 写入；常与掩膜扫描累加器有关，`docs/archive/ai/74` §1.1 实测常规对局恒 0） | `0x51b114` |

**个体**（`0x30` = 48 字节/个，`0x51bccf` 的 `lea (%r14,%r14,2); shl $4` 就是 `×48`）：

| 偏移 | 内容 | 写入点（克隆路径 / 工厂路径） |
|---|---|---|
| `+0x00` ptr / `+0x08` len / `+0x10` cap | `Vec<Token>` | `0x51bcd7`–`0x51bce0` / `0x51c10d`–`0x51c116` |
| `+0x18` f64 | sret `+0x00`（**排序键**） | `0x51bce5`–`0x51bcee` / `0x51c11b`–`0x51c124` |
| `+0x20` f64 | sret `+0x08`（距离项） | 同上（`movdqu` 一次搬 16 字节） |
| `+0x28` u32×2 | sret `+0x10`/`+0x14` | `0x51bcfc` / `0x51c132` |

排序比较器读的就是 `+0x18`（`docs/95` §2.5：插入排序版 `0x41b500` 直接读死降序）。
**判据读的却是 `+0x20` 和 `+0x28`** —— 这是本轮最容易被搞错的一处。

---

## 3. 判据：逐条反汇编（第 0 名，另两名同构）

先看基准是怎么存下来的（`0x51c2b5`–`0x51c2c7`）：

```asm
51c2b5: movupd 0x18(%rbp),%xmm0     ; rbp = 个体数组基址；[+0x18]=score, [+0x20]=dist
51c2ba: mov    0x28(%rbp),%rax      ; 个体 +0x28 = {hits, _}
51c2be: lea    0x28(%rsp),%rcx
51c2c3: mov    %rax,0x10(%rcx)      ; 栈 0x38(%rsp) = 粗评 hits
51c2c7: movupd %xmm0,(%rcx)         ; 栈 0x28(%rsp)=score、0x30(%rsp)=粗评 dist
```

精评调用（`0x51c39a`–`0x51c3b3`）。sret 基址是 `0xf8(%rsp)`，于是
`sret+0x00 = 0xf8`、`sret+0x08 = 0x100`、`sret+0x10 = 0x108`
⇒ 下面读的正是 **`sret+0x10`（hits）** 与 **`sret+0x08`（dist）**：

```asm
51c39a: lea    0xf8(%rsp),%rdi      ; sret
51c3a2: lea    0x10(%rsp),%rdx      ; 已加过修正的 token 向量 (f + c)
51c3a7: mov    0x4c(%rsp),%ecx      ; ctx 缓存 key（param_4）
51c3ab: movsd  0x19f110,%xmm0       ; = 0.01
51c3b3: call   0x51a750             ; 评分核心
```

裁决（**这就是"不一致"的完整判据**）：

```asm
51c3b8: mov    0x108(%rsp),%eax     ; eax = 精评 sret+0x10 = hits
51c3bf: cmp    0x38(%rsp),%eax      ; 与粗评 hits（个体 +0x28）
51c3c3: jne    0x51c3ea             ; ① 不同 → **不一致**，跳到换新
51c3c5: movsd  0x100(%rsp),%xmm0    ; xmm0 = 精评 sret+0x08 = dist
51c3ce: subsd  0x30(%rsp),%xmm0     ; − 粗评 dist（个体 +0x20）
51c3d4: andpd  0x19c050,%xmm0       ; 取绝对值（0x19c050 = 0x7fff_ffff_ffff_ffff）
51c3dc: ucomisd 0x19f6d0,%xmm0      ; 与 0.5 比较（**复用粗评步长那个常量**）
51c3e4: jbe    0x51c499             ; ② |Δ| <= 0.5 → **一致**，什么都不做
51c3ea: <换新>                       ; ③ 否则 → 换新
```

要点：

* **不是**"精评分数与粗评分数差 > 阈值"。被比的是 `dist`（sret `+0x08`），
  不是 `score`（sret `+0x00`）。`docs/95` §2.7 里"长度变了，或 |Δscore| > 0.5"
  的括注**两处都不对**（长度根本没进比较；比的是距离项），本文更正。
* 阈值 `0.5` 与粗评步长是**同一个 rodata 常量** `0x19f6d0`
  （`0x51c3dc` 与 `0x51bc8b`/`0x51c0c1` 读的是同一个地址）。
* `ucomisd` + `jbe`：有序时等价于 `|Δ| ≤ 0.5`；无序（NaN）时 `ucomisd` 置
  ZF/CF/PF，`jbe` **成立** ⇒ **NaN 算"一致"**（实现里用 `!(… > tol)` 而不是
  `… <= tol` 才等价）。
* `hits` 这一支在 `|Δscore| > 0.5` 时也会触发（命中数变 1 个 ⇒ 分数差 2e6），
  但反过来不成立 ⇒ 两支都要实现。
* 三名各有一份完全同构的代码，基准读取点分别是
  `0x51c2b5`(名次 0，`rbp+0x18/+0x28`)、`0x51c5f0`/`0x51c5fd`(名次 1，`rbp+0x48/+0x58`)、
  `0x51c8a7`/`0x51c8b7`(名次 2，`rbp+0x78/+0x88`)。

### 3.1 `hits` / `dist` 到底是什么（评分核心 `0x51a750`）

```asm
51b0ad: cvtsi2sd %eax,%xmm1 ; 51b0b1: mulsd 0x19fc00(=2e6)   ; hits 项
51b0d9: subsd    0x8(%rsp),%xmm1                            ; − dist
51b0ef: subsd    %xmm3,%xmm1                                ; − V²/100
51b0fb: cmp %rcx,%rdx / 51b100: addsd 0x19f478(=-5e7)       ; V > limit 惩罚
51b111: mov  %eax,0x10(%rcx)          ; hits → sret+0x10
51b11d: movsd 0x8(%rsp),%xmm0 → +0x8  ; dist → sret+0x08
```

* `hits`：`0x51ae4c dec %eax`（友方）/ `0x51ae5d inc %eax`（敌方）累加的那个计数器
  （清零点 `0x51ab42`，敌方/友方判定 `0x51ae21`–`0x51ae4a`），最后落到 `eax`。
  ⇒ "命中数项 = 敌人数 − 友军数"，就是适应度里乘 2e6 的整数（`0x51b0ad`–`0x51b0b9`）。
* `dist`：栈槽 `0x8(%rsp)`。初值 `0x51ab2d`/`0x51afdf` 从 `0x19fb30` 取 **1e6**，
  循环里只有一处会改它：`0x51aed6 movsd 0x8(%rsp),%xmm1 / 0x51aedc ucomisd / 0x51aee0 jbe /
  0x51aee2 movsd %xmm0,0x8(%rsp)` —— 即 **取 min**，且只在某个事件分支里做
  （`0x51aeb8`–`0x51aed4` 的类型判定之后；结构体 32 字节、带 tag 的跳转表在 `0x1c63a0`）。

⚠️ **转引实测（`docs/archive/ai/74` §1/§2）**：常规对局里 `dist` 恒为 1e6 —— 全未命中约 −1e6、
命中一人约 +1e6（= `2e6 − 1e6`）、命中两人约 +3e6。也就是说**那个 min 分支在常规
对局里不触发**，距离判据实际是"死"的，真正起作用的只有 `hits` 这一支。
**推断**（未证实）：该分支服务于另一种目标类型（tag 判定里要求某种 kind == 2）。

---

## 4. 换新：换谁、从哪来、RNG

第 0 名（`0x51c3ea` 起；第 1/2 名是 `0x51c725` 起、`0x51c9df` 起，逐指令同构）：

```asm
51c3ea: movzbl 0x7(%rsp),%ebx       ; 函数模式（Normal/DiffEq）字节
51c3f6: call   0x4ef020             ; ① makeRandomFunction(mode)
51c40a: call   0x4ee0e0             ; ② 构造/补齐（docs/AI.md §6，rand%50 那一套）
51c44c: call   0x4ee720             ; ③ 变异（docs/AI.md §7）
51c451: cmpq   $0x0,0xb0(%rsp)      ; 守卫：len > rank 才动手（0/2/3 三档）
51c468: mov (%rbx),%rsi / 51c470: mov 0x8(%rbx),%rdi / 51c47d: call *%r13   ; 释放旧 token
51c480: mov 0xf0(%rsp),%rax → 0x10(%rbx)     ; 新 len
51c495: movupd 0xe0(%rsp),%xmm0 → (%rbx)     ; 新 ptr/cap
```

* **换的是该名次本身**：写回目标分别是 `rbp+0x00`、`rbp+0x30`（`0x51c7bc`）、
  `rbp+0x60`（`0x51ca76`）⇒ 第 0/1/2 名。不是"踢掉再补到队尾"。
* **新的从哪来**：与槽循环的随机分支（`0x51b90f` / `0x51b923` / `0x51b95c`）
  **同三个调用、同顺序**：`4ef020 → 4ee0e0 → 4ee720`。即"随机函数 + 补齐 + 变异"。
* **RNG 消耗**：与"随机新生一条"完全相同 —— 一次 `random_length`（1 次
  `normal()`）+ 每个 token 一次 `random % 85` 抽样（常数再加一次 `normal()`）
  + 补齐时的 `rand % 50` 循环 + 变异的一次抽样
  （抽样粒度见 `src/_scaffold/rng.rs` 的 `next_u32`；反汇编 `0x4ef193`）。
  第 1/2 名也各来一遍 ⇒ 一代最多 **3 条**随机函数。
* **分数字段不更新**：只覆写 `Vec` 的 24 字节，`+0x18/+0x20/+0x28` 留着**旧粗评分**。
  ⚠️ 后果（观测到的代码行为，不是 bug 报告）：若这一代正好是最后一代，
  返回数组里被换掉的那几条，其 `score` 与 `tokens` 不再对应。
* **一致时不写回**：`0x51c3e4 jbe 0x51c499`，`0x51c499` 起只有释放临时 token 向量的代码。

---

## 5. 为什么是 3

* 三名是**三段展开**的代码（`0x51c3b3`、`0x51c6ee`、`0x51c9a8`），**没有**"3"这个立即数，
  也没有循环计数器 ⇒ 不是"前 N 名"的参数化实现。
* 三名各带一个"数组够不够长"的守卫：`0x51c451 cmpq $0x0`（第 0 名要 `len ≥ 1`）、
  `0x51c78c cmpq $0x2`（第 1 名要 `len ≥ 2`）、`0x51ca46 cmpq $0x3`（第 2 名要 `len ≥ 3`）。
  第三名之后直接进代末尾巴 `0x51cabe`。
* 没有配置字段控制它：`FUN_0061b440` 的 6 个整型实参里
  （`cfg+0x6c/0x70/0x74/0x78`，`docs/95` §1）没有一个落到这三个块上，
  三个守卫都是**立即数**。
* ⇒ **3 是硬编码的。为什么是 3（设计意图）—— 未知**：代码只给出"3"，
  给不出理由。可以说的只有"精评只花 3 次评分，相对每代 `种群+3` 的预算几乎免费"
  （`docs/archive/ai/67` 的运行时计数就是 `代数 × (种群+3)`）。**这是推断，不是结论。**

---

## 6. 粗评的范围：**全部**，克隆也重评

* 槽循环每槽先放一个克隆（`0x51b8c5` memcpy，**只复制 token**），随后
  `0x51b8fa jmp 0x51b961` 进入同一个"复制 token 向量 → 修正 → 粗评 → 写回"流程
  （`0x51b993` / `0x51bbaa` / `0x51bc8b` / `0x51bcc7`）⇒ **克隆的分数是新算的**，
  不是继承父代。
* 工厂子代在 `0x51c0c1` 上走同一套。
* ⇒ 每代 `5s = 种群` 次粗评 + 3 次精评。与 `docs/archive/ai/67` 的运行时实测
  "每回合 = 代数 × (种群 + 3)"（难度 1–5 实测 `63250 = 250 × 253`）**吻合**；
  本仓库新增的 `each_generation_coarsely_scores_everyone_plus_three_fine` 测试把这条钉住。

---

## 7. 等价伪代码

```rust
// 一代（= 二进制 FUN_0061b440 的循环体）
fn generation(prev: &[Individual], p: &Params) -> Vec<Individual> {
    let s = p.population / p.per_slot;        // 20%
    let mut born = Vec::with_capacity(p.population);

    for slot in 0..s {
        // ① 克隆（slot < prev.len() 时；否则随机新生）
        let mut c = prev[slot].clone();       // 只复制 token（0x51b8c5）
        c.eval = score(&c.tokens, COARSE);    // ★ 克隆也要粗评（0x51bc8b）
        born.push(c);

        // ② per_slot − 1 个工厂子代，每个都粗评（0x51c0c1）
        for _ in 1..p.per_slot {
            let t = factory(prev);            // 变异 / 重生 / 交叉+变异（0x5ee3a0）
            born.push(Individual::scored(t, score(&t, COARSE)));
        }
    }

    // ③ 降序排序 + 截断到 s（0x51cb6f / 0x51c208）
    born.sort_desc_by(|i| i.score);           // 比较器读个体 +0x18
    born.truncate(s);

    // ④⑤ 前 3 名精评 + 纠偏（0x51c3b3/0x51c6ee/0x51c9a8）
    for rank in 0..min(3, born.len()) {
        let fine = score(&born[rank].tokens, FINE);      // step = 0.01
        let ok_hits = fine.hits == born[rank].hits;
        let ok_dist = !(|fine.dist − born[rank].dist| > 0.5);
        if ok_hits && ok_dist {
            continue;                                    // 一致：不写回
        }
        let t = mutate(repair(make_random_function(mode)));  // 4ef020→4ee0e0→4ee720
        born[rank].tokens = t;                            // 只换函数，分数留旧值
    }
    born
}
```

`score(tokens, phase)` = 评分核心 `0x51a750`，两阶段**同一个函数**：
唯一差别是 `xmm0 = phase.step()`（0.5 / 0.01）；
进入前两边都会先算修正 `c = y0 − f(±x0)` 并把 token 变成 `(+) f c`
（粗评 `0x51bbde`/`0x51bc21`/`0x51bc5e`，精评 `0x51c304`/`0x51c344`/`0x51c37e`）。
`ctx+0x5c != 0` 时两边都**跳过**修正（`0x51bbaa` / `0x51c2d0`）。

---

## 8. 实现（`src/graphwar_function_evolutionator.rs`）与对原版的**有意偏差**

新增（全部带地址注释）：

| 名字 | 含义 |
|---|---|
| `Phase::{Coarse, Fine}` + `Phase::step()` | 0.5 / 0.01 |
| `COARSE_STEP` / `FINE_STEP` / `FINE_RANKS` / `CONSISTENCY_TOL` / `DIST_SENTINEL` | 常量，带来源地址 |
| `Evaluation { score, dist, hits }` | 评分结果三元组（sret 的前 24 字节） |
| `Evaluation::consistent_with(&coarse)` | 判据，逐条对应 `0x51c3b8`–`0x51c3e4`（含 NaN 语义） |
| `Individual { tokens, score, dist, hits }` | 个体 48 字节布局 |
| `fitness_eval()` / `ScoringContext::evaluate()` / `with_step()` | 两阶段评分器 |
| `scalar_scorer()` | 把老的 `FnMut(&[Token]) -> f64` 适配成两阶段 |
| `fine_pass()`（私有） | 代末前 3 名精评 + 换新 |

**偏差 1（`dist` 的活跃度）**：`docs/archive/ai/74` 的运行实测表明原版常规对局里 `dist` 恒 1e6
（min 分支不触发）。我们如实算"命中点到目标圆心的距离"（`dist ∈ [0, radius]`），
于是距离判据**更活跃**：radius = 0.5 时 `|Δ| ≤ 0.5` 恒成立 ⇒ 实际仍然不触发，
这一点与原版一致；若目标半径 > 0.5，我们的版本会多触发。要逐位对齐原版，
把 `dist` 恒置 `DIST_SENTINEL` 即可（`Evaluation::scalar` 就是这个形态）。
**证据等级**：判据 = 观测；`dist` 语义 = 推断；"原版常规对局不触发" = 转引实测。

**偏差 2（返回值）**：`next_generation` 现在返回 **s = 种群/per_slot 个存活者**
（二进制里截断后数组就只剩 s 个，下一代按槽号取 `prev[0..s]`），
改动前返回整代 `种群` 个。`evolve` 的返回值也随之变成存活者。

**顺带发现的、本轮没改的东西**（留待父 agent 决定）：

1. `random_function()` 内部调 `repair(..., VarMode::Normal, next)` **写死了 Normal**
   （`src/graphwar_function_evolutionator.rs`），DiffEq 模式下补齐会补出 `x` 而不是 `y`。与两阶段无关，未动。
2. 槽循环的"随机分支"（我们这边是 `sorted[b].tokens.len() < 20` 那支）在二进制里
   **还会调一次变异**（`0x51b95c call 0x4ee720`），改动前的 Rust 没有。
   本次只在"换新"路径里补上了 `mutate`（因为那一段的二进制证据就是三条调用）。
3. `docs/95` §2.7 的括注（"长度变了，或 |Δscore| > 0.5"）应改成
   "命中数变了，或 |Δ距离项| > 0.5"；`docs/AI.md` §5 末尾建议补上"代末收尾"这一段。

---

## 9. API 改动与迁移

```rust
// 旧
pub fn next_generation<F>(pop: &[Individual], p: &Params, mode: VarMode,
                          next: &mut dyn FnMut() -> u64, score: F) -> Vec<Individual>
where F: FnMut(&[Token]) -> f64;                       // 返回整代 种群 个

// 新
pub fn next_generation<F>(pop: &[Individual], p: &Params, mode: VarMode,
                          next: &mut dyn FnMut() -> u64, score: F) -> Vec<Individual>
where F: FnMut(&[Token], Phase) -> Evaluation;         // 返回 种群/per_slot 个存活者
```

`evolve` / `evolve_with_budget` 的 `score` 同样从 `FnMut(&[Token]) -> f64`
变成 `FnMut(&[Token], Phase) -> Evaluation`；`Individual` 多了 `dist`/`hits` 两个字段
（构造用 `Individual::new()` / `Individual::with_evaluation()`）。

迁移三行：

```rust
// 1) 只关心分数、不关心阶段（测试里最常见的写法）：
let out = evolve(pop, &p, mode, &mut next, scalar_scorer(|t: &[Token]| t.len() as f64));

// 2) 接真适应度：一行就够，ctx 自己按阶段换步长
let out = evolve(pop, &p, mode, &mut next, |t: &[Token], ph| ctx.evaluate(t, ph));

// 3) 只想要一个分数（老代码）：
let s = graphwar_function_evolutionator::fitness(tokens, &ctx);        // = fitness_eval(..).score，签名未变
```

`examples/replay_compare.rs` 已按第 2 种改。

---

## 10. 证据等级表

| # | 结论 | 证据 | 等级 |
|---|---|---|---|
| 1 | 每代所有个体（含克隆）粗评 step 0.5 | `0x51bc8b`、`0x51c0c1`；克隆经 `0x51b8c5`→`0x51b993`→`0x51bc8b` | **观测** |
| 2 | 代末降序排序 + 截断到 s | `0x51c1e2`/`0x51cb6f`/`0x51c208`（`docs/95` §2.5–2.6） | **观测** |
| 3 | 前 3 名精评 step 0.01 | `0x51c3ab`/`0x51c6e6`/`0x51c9a0` + 三道 `cmpq` 守卫 | **观测** |
| 4 | 判据 = `hits` 不等 ∨ `|Δdist| > 0.5` | `0x51c3b8`–`0x51c3e4`（另两名同构） | **观测** |
| 5 | 精评分数不写回 | 一致分支 `0x51c3e4 jbe 0x51c499` 只有释放代码；不一致分支只覆写 24 字节 | **观测** |
| 6 | 换新 = 随机函数 + 补齐 + 变异，与随机分支同构 | `0x51c3f6`/`0x51c40a`/`0x51c44c` vs `0x51b90f`/`0x51b923`/`0x51b95c` | **观测** |
| 7 | 换的是第 0/1/2 名本身 | 写回偏移 `+0x00`/`+0x30`/`+0x60` | **观测** |
| 8 | 3 是硬编码、非配置 | 三段展开 + 三个立即数守卫 + 无配置实参 | **观测**（负结果，范围已写明） |
| 9 | 两阶段共用 `0x51a750`，只差 `xmm0` | 五个调用点、同一 sret、同一修正预处理 | **观测** |
| 10 | `hits` = 乘 2e6 的那个整数（敌−友） | `0x51b0ad`–`0x51b111`；`0x51ae4c dec`/`0x51ae5d inc` | **观测** |
| 11 | `dist` = 距离项、初值 1e6、取 min、被减 | `0x51ab35`/`0x51afdf`、`0x51aed6`–`0x51aee2`、`0x51b0d9`、`0x51b11d` | **观测** |
| 12 | 常规对局 `dist` 恒 1e6 ⇒ 距离判据实际不触发 | `docs/archive/ai/74` §1/§2 的运行时分布 | **转引实测** |
| 13 | 那个 min 分支什么时候才更新（哪种目标类型） | 跳转表 `0x1c63a0` + `0x51aeb8`–`0x51aed4` 的类型判定，未逐条解 | **未知** |
| 14 | "为什么是 3"（设计意图） | 代码只有 3 | **未知** |
| 15 | `sret+0x14`（`esi`）的语义 | 与掩膜扫描累加器相关（`docs/archive/ai/74` §1.1 实测恒 0） | **推断**（不入判据，无影响） |

---

## 11. 复现命令

```bash
cd /home/user/MyProject/gw2
# 判据（第 0 名）
objdump -d --start-address=0x51c3b0 --stop-address=0x51c3f0 libgraphwarrust.so
# 另外两名同构
objdump -d --start-address=0x51c6f0 --stop-address=0x51c730 libgraphwarrust.so
objdump -d --start-address=0x51c9a8 --stop-address=0x51c9e0 libgraphwarrust.so
# 五处评分调用（2 × 0.5 + 3 × 0.01）
objdump -d --start-address=0x51bc80 --stop-address=0x51bca0 libgraphwarrust.so
objdump -d --start-address=0x51c0b0 --stop-address=0x51c0d0 libgraphwarrust.so
# 常量
python3 -c "import struct;d=open('libgraphwarrust.so','rb').read();\
print([struct.unpack('<d',d[o:o+8])[0] for o in (0x19f6d0,0x19f110,0x19fb30,0x19fc00,0x19f478,0x19c050)])"
# ⇒ [0.5, 0.01, 1000000.0, 2000000.0, -50000000.0, nan]   (nan = 0x7fff_ffff_ffff_ffff 绝对值掩码)
# 本仓库测试
export CARGO_HOME=$PWD/../.cargo-home CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo test --offline -p gw2-rebuild aigen
```
