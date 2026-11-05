# 99 · 代与代之间种群怎么维持：父代范围 / 重评分 / 两个变体

状态：结论性（三问三答）。本轮只做**静态**分析（REA/Ghidra 12.1.4 反编译 + GNU objdump 2.47），
另外**转引**了他人已落在 `.rea-work/` 的运行时数据做交叉验证（§6，来源与局限都写在那一节）。
未改 `src/`，未跑 `cargo test`。

地址约定同全仓：**Ghidra 地址 = ELF vaddr + 0x100000**。本文正文一律写 **vaddr**，
关键函数另给 Ghidra 地址；objdump 输出就是 vaddr。
（`docs/12`、`archive/ai/74`、`docs/87` 把 vaddr 写成 "Ghidra 地址" 的老坑，见 `docs/AI.md` §8.6。）

---

## 0. 三句话结论

| # | 问题 | 结论 |
|---|---|---|
| ① | 抽父代的取值范围？ | **不是**在整个种群均匀抽。每代有 `s = 种群/5` 个槽位，**槽 i 只以"上一代排序后第 i 名"为父代**：1 个精确克隆 + `param_7−1 = 4` 个子代，**4 个子代共用同一个父代**。所以父代范围 = **存活槽（上一代前 s 个 = 前 20%）**，且每个存活个体**恰好当 1 次父代**。"两个父代"在游戏里不存在 ⇒ "允许同一个体当两个父代"这个问题**不成立**（只有 1 个父代）。`slot ≥ 上一代长度 → 随机重生` 只在**第 1 代**发生（初始种群是空 Vec，实测见 §6）。 |
| ② | 克隆沿用旧分数还是重算？有缓存吗？ | **每代全量重算，没有缓存、也没有"分数有效位"**。克隆只复制 token 数组（`cap/ptr/len` 三个字），个体结构里 `+0x18..+0x30` 的 24 字节评分元组**根本不复制**；每代 `5s` 个个体全部重新粗评（步长 0.5），再加**前 3 名各 1 次**精评（步长 0.01）⇒ **每代 `种群 + 3` 次评分**，总计 `代数 × (种群+3)`。精评**不写回分数**，它只是"粗评可不可信"的校验；校验不过就把该条**整条换成"随机函数+一次变异"并保留原来的旧分数**。 |
| ③ | 什么时候用变体 B？ | `param_15` = 配置结构 `+0x80` 的一个 bool，来源是**模块容器单例对象 `+0x32` 字节**（vaddr `0x479af0`–`0x479af4`）。`0 → A`（`0x51b440`，种群 GA，返回 s 个存活者）；`非 0 → B`（`0x51d080`，**(1+λ) 单点爬山**：只维持 1 个基函数，每代造 λ 个变体、排序、精评校验后取第一个通过的当新基，最终**只返回 1 个个体**）。**运行时实测走的是 A**（§6）。 |

---

## 1. 证据等级约定

| 等级 | 含义 |
|---|---|
| **观测** | 能指到具体地址/字节，本轮自己核过 |
| **观测（转引）** | 别人落在 `.rea-work/` 的运行时数据，本轮只做算术一致性核对 |
| **推断** | 有代码依据但未证实（写明缺什么） |
| **未知** | 拿不到 |

---

## 2. 变体 A（vaddr `0x51b440` / Ghidra `0x61b440`）：一代的完整结构

### 2.1 入口签名（实参全部来自配置结构）

```
FUN_0061b440(out, ctx, p3, key, 代数, s, 每槽个体数, &初始种群Vec)
              │    │    │    │     │    │      │            └ cfg+0x28 的 Vec{cap,ptr,len}
              │    │    │    │     │    │      └ cfg+0x78（运行时恒 5，见 docs/95）
              │    │    │    │     │    └ cfg+0x74 = 种群/5 = 10/20/30/40/50
              │    │    │    │     └ cfg+0x70 = 3/10/25/100/250/1000
              │    │    │    └ cfg+0x6c = 士兵 id（查 x0,y0 与镜像位的哈希表 key）
              │    │    └ cfg+0x68（A 里**未被使用**，死参数）
              │    └ 由 FUN_0061eb90 建出的 ctx（含 limit@+0x58、函数模式@+0x5c）
              └ 返回 Vec<个体>（长度 = s）
```

取参现场：`0x51b451 mov [rsp+0x11c],r9d`（s）、`0x51b459 mov [rsp+0x118],r8d`（代数）、
`0x51b461 mov [rsp+0x4c],ecx`（key）、`0x51b77f mov eax,[rsp+0x1c0]`（第 8 实参=每槽个体数）。
配置字段→实参的搬运在 `FUN_00538780`（vaddr `0x438780`，Ghidra `0x538780`，0xB0 字节）。

### 2.2 每个个体长什么样（48 字节，本轮新确定）

| 偏移 | 内容 | 证据 |
|---|---|---|
| `+0x00` | token Vec 的 **cap** | 释放时 `mov rsi,[r15-0x8]; shl rsi,4` 当字节数用（`0x51cae9`、`0x51d05d`） |
| `+0x08` | token Vec 的 **ptr** | `0x51b874`（克隆取指针）、`0x51caf2`（释放） |
| `+0x10` | token Vec 的 **len**（token 数，×16 字节） | `0x51b845`（克隆取长度）、`0x51c222` |
| `+0x18` | **f64 排序键 = 分数** | 排序比较器 `0x41b572`/`0x41b577` 直接 `movsd [rdx±0x18]`；评分核 `0x51b10d movsd [rcx],xmm1` |
| `+0x20` | f64，第二分量（精评一致性判定用） | `0x51c3c5–0x51c3dc`；评分核 `0x51b11d movsd [rcx+8],xmm0` |
| `+0x28` | u32 + u32，两个计数分量（第二个只写不读） | 评分核 `0x51b111/[+0x10]←eax`、`0x51b114/[+0x14]←esi` |

> 所以"个体"里**没有任何 1 个 bit 表示"分数还有效"**。

### 2.3 一代的等价伪代码

```c
// —— 进循环前只做一次 ——
(x0, y0) = 哈希表[ctx->inner][key];          // 0x51b591 起（内联的哈希探测）
mirror   = 哈希表2[ctx->inner2][key];        // 0x51b6df 附近；为真则 x0 ← −x0
t0 = Instant::now();                         // 0x51b71e / 0x51b724
prev_ptr = init_vec.ptr; prev_len = init_vec.len;      // 0x51b713/0x51b71a

for (gen = 0; gen != 代数; gen++) {           // 0x51b7c6 循环头
    if (elapsed(t0) > 25s) break;             // 0x51b7cd–0x51b7e4（docs/93）

    new_pop = Vec::new();                     // 0x51b7ea–0x51b802
    for (slot = 0; slot != s; slot++) {       // 0x51b829 … 0x51bda7

        /* ---- (a) 取父代：只有 1 个，就是 prev[slot] ---- */
        if (slot < prev_len) {                           // 0x51b829 cmp;0x51b82c jbe
            cur = deep_copy(prev[slot].tokens);          // 0x51b832 槽号×48, 0x51b8c5 memcpy
        } else {                                         // 0x51b900 随机重生分支
            cur = makeRandomFunction(mode);              // 0x51b90f call 0x4ef020
            cur = assemble(cur, mode);                   // 0x51b923 call 0x4ee0e0
            cur = mutate(cur);                           // 0x51b95c call 0x4ee720
        }
        // ↑ 到这里 cur 就是本槽的"唯一父代"，后面 4 个子代全部从它出

        /* ---- (b) 本槽第 0 个个体 = 克隆本身（也要打分） ---- */
        if (每槽个体数 != 0) {                            // 0x51b968
            f  = fold_constants(cur);                    // 0x51b993 call 0x4ebbe0
            if (len(f) > ctx->limit) f = repair(f);      // 0x51ba3c..0x51ba47（ctx+0x58 = limit）
            if (mode == Normal)                          // ctx+0x5c == 0
                f = f + const(y0 - eval(f, x0));         // 0x51bbde eval, 0x51bc21..0x51bc70 追加 tag3/0x11
            sc = score(f, step=0.5, key);                // 0x51bc8b/0x51bc93 call 0x51a750
            new_pop.push({f, sc});                       // 0x51bcd7..0x51bd04
        }

        /* ---- (c) 本槽剩下 param_7−1 个：工厂子代，父代还是 cur ---- */
        for (k = 0; k != 每槽个体数-1; k++) {            // 0x51bd5f / 0x51c19c dec
            p  = deep_copy(cur);                         // 0x51bdb2..0x51be75
            ch = factory(mode, p);                       // 0x51be93 call *0x9bc288 → vaddr 0x4ee3a0
            ch = fold_constants(ch); … 超 limit 修补 …
            if (mode == Normal) ch = ch + const(y0 - eval(ch, x0));
            sc = score(ch, step=0.5, key);               // 0x51c0b0/0x51c0c9
            new_pop.push({ch, sc});                      // 0x51c10d..0x51c13a
        }
    }   // 一代共 5s 个个体，5s 次粗评

    /* ---- (d) 代末：降序排序 + 截断到前 s ---- */
    if (len(new_pop) >= 2)
        sort_by_score_desc(new_pop);                 // 0x51c1e2; <21 → 0x41b500，否则 0x4605f0
    new_pop.len = min(new_pop.len, s);               // 0x51c1f8–0x51c210（多出来的 4s 个被释放）

    /* ---- (e) 前 3 名用 0.01 精评校验；不一致就整条换掉 ---- */
    for (r = 0; r != 3 && r < len(new_pop); r++) {   // 三段展开：0x51c3b3 / 0x51c6ee / 0x51c9a8
        sc2 = score(new_pop[r].f, step=0.01, key);
        if (sc2.f10 != new_pop[r].f10 ||              // 0x51c3b8–0x51c3c3
            |sc2.f08 - new_pop[r].f08| > 0.5) {        // 0x51c3c5–0x51c3e4（0.5 复用粗评步长常量）
            new_pop[r].f = mutate(makeRandomFunction(mode));  // 0x51c3ea..0x51c451
            /* ★ 只改写 cap/ptr/len，+0x18 的旧分数**不改** ⇒ 分数位带着旧值 */
        }
    }   // 精评只做判定，不写回分数

    prev_ptr = new_pop.ptr; prev_len = new_pop.len;   // 0x51cb31/0x51cb36/0x51cb39/0x51cb41
    /* 旧种群在这里整体释放 0x51cabe–0x51cb2c */
}
*out = prev;   // 返回长度 s 的 Vec；0x51cbaf 是"跑满"和"超时"的共用出口
```

### 2.4 判据：为什么"父代 = 上一代第 slot 名"

- 槽 i 的克隆源是 `prev[i]`，下标就是槽号：`0x51b832 lea rax,[r14+r14*2]; 0x51b836 shl rax,0x4`
  ⇒ 字节偏移 = `槽号 × 48`；再 `0x51b83a mov rsi,[rsp+0xb8]`（基址）+`0x51b845/+0x51b874` 取 len/ptr。
- `prev` 每代都被换成"排序后截断到 s"的那个 Vec（`0x51cb31–0x51cb41` 把新 Vec 的 len/ptr/cap 写回
  `r12`/`[rsp+0xb8]`/`[rsp+0xc8]`，而 `r12` 正是下一轮 `0x51b829` 比较用的"上一代长度"）。
- 排序方向 = **降序**（按 `+0x18` 的 f64）：比较器 `0x41b572 movsd xmm0,[rdx-0x18]`、`0x41b577 movsd xmm3,[rdx+0x18]`、
  `0x41b57c ucomisd` / `0x41b586 ucomisd xmm3,xmm0; jbe`（NaN 走 panic）。所以"第 i 名"= 第 i 好的。
- 4 个工厂子代拿到的父代是 `cur` 的**常量折叠副本**（每个子代各折一次：`0x51bdd7 call 0x4ebbe0`，
  再把折叠结果 `memcpy` 一份（`0x51be5c`），最后 `0x51be93` 传给工厂）；克隆本身也先折叠再评分（`0x51b993`）——
  所以进入评分与进入工厂的都是**折叠后**的表达式，`cur` 自己不被改写，
  工厂的签名只有 3 个参数（`out, &mode, &parent`），**没有种群、没有第二个父代**。

### 2.5 工厂 `0x4ee3a0`（Ghidra `0x5ee3a0`）内部（复核 `docs/AI.md` §5）

```c
factory(out, mode, parent) {
    r = next_i32();                       // FUN_00850180 线程 RNG 缓冲（64 个 i32 一批）
    if (r < 0) return mutate(parent);     // vaddr 0x4ee720  ← 约 50%
    base = (parent.len < 20) ? makeRandomFunction(mode) : empty;   // vaddr 0x4ef020
    r1 = next() % parent.len;                        // 0x…（两次取随机）
    r2 = next() % (parent.len - r1);
    sub = parent.tokens[r1 .. r1+r2];                 // 连续区间
    piece = take_one_complete_subtree(sub);           // FUN_004be7d0 = vaddr 0x3be7d0（按前缀表达式递归取一个完整子式）
    return assemble(base ++ piece, mode);             // vaddr 0x4ee0e0（装配/补齐）
}
```
⇒ **单亲 + 取子树 + 变异**；`docs/AI.md` §5 的"只从一个父代取一段连续子串"是对的，
更准确的说法是"取一个**完整子表达式**（操作数对齐），不是任意字节区间"。

---

## 3. ① 父代选择范围 —— 逐条回答

| 子问题 | 答案 | 依据 |
|---|---|---|
| 取值范围是"全部 n"吗？ | **不是**。没有任何"在整个种群上取模抽下标"的代码：驱动里对 `prev` 的**唯一**下标就是槽号本身（`槽号×48`），而槽号 `0..s−1` 覆盖的正是截断后的前 s 个。 | `0x51b832/0x51b836`、`0x51c208` |
| 是"前 s 个精英"吗？ | **是**，且是**一对一**的（槽 i ↔ 第 i 名），不是在前 s 里再随机抽。 | 同上 + 降序排序 `0x41b57c` |
| 是"存活槽"吗？ | **是**，两种表述等价：`prev` 的长度就是上一代存活数（= s），槽号 `< prev_len` 才克隆。 | `0x51b829 cmp r12,r14` + `0x51cb36 mov r12,…` |
| `0x51b829` 的随机重生分支什么时候走？ | 只有 `槽号 ≥ 上一代长度` 时。运行时**通常只在第 1 代**走（初始种群 Vec 长度 0）⇒ 第 1 代 = `s` 个"随机函数 + 一次变异"，**没有父代**。
   ⚠️ **更正（`docs/110`）**："只在第 1 代"只在"缓存里没有该 key 的存量"时成立。初始种群其实是**从一张哈希表 move 出来的 `Vec`**，可以非空；非空时第 0 代是**混合**的（前几槽克隆存量、其余随机）。另外随机分支是**三次调用**（随机函数 → 截断补齐 → 变异），结果是该槽 5 个个体的**共同父代**，不是"没有父代"。 | `0x51b82c jbe 0x51b900`；运行时 §6 |
| 一个子代有几个父代？ | **1 个**。 | 工厂签名 `0x51be84 lea rdi,out / 0x51be89 lea rsi,&mode / 0x51be8e lea rdx,&parent`；`FUN_005ee3a0` 只读 `param_3` |
| 允许同一个体当两个父代吗？ | **问题不成立**（没有第二个父代）。但**同一个父代会被同一个槽的 4 个子代共用**，这一点与我们"两个不同个体"的实现正好相反。 | 同上 |

**结论对复现的影响**：我们把 `a = next()%len`、`b = next()%len` 换成"**槽 i 用 survivor[i]，4 次**"之后，
搜索的"选择压力"性质会变（游戏是 1 个父代 → 4 个变异体，我们原来是 2 个父代混合 → 更接近经典交叉；
游戏的"交叉"发生在**同一个体内部的两块子树之间**）。

---

## 4. ② 精英重评分 / 缓存失效

### 4.1 每代评分次数

| 项 | 次数/代 | 证据 |
|---|---|---|
| 粗评（步长 0.5，`0x19f6d0`） | **`5s` = 种群** | 两个调用点：`0x51bc93`（槽内第 0 个）、`0x51c0c9`（工厂子代），都在槽循环内 |
| 精评（步长 0.01，`0x19f110`） | **3**（前 3 名，`len<3` 时更少） | `0x51c3b3`、`0x51c6ee`、`0x51c9a8` |
| **合计** | **`种群 + 3`** | 与 `docs/93` §3 的表一致 |

`objdump` 全文里 A 的函数体内 `call 51a750` **恰好 5 条**（2 粗 + 3 精），没有第六条。
比例：`每代评分 / 种群 = 1 + 3/种群`；`总计 = 代数 × (种群+3)`。
难度 6 = `1000 × 253 = 253000`；难度 5 = `250 × 253 = 63250`。

每代另有（不进评分计数，但都是实打实的开销）：
`4s` 次工厂调用（`0x51be93`）、`5s` 次常量折叠（`0x4ebbe0`）、`5s+3` 次求值器调用（`0x4eb9d0`，**仅 Normal 模式**，用来算修正项）。

### 4.2 有没有"分数有效位"？—— 没有

- 克隆**只**复制 token 数组：`0x51b832–0x51b8c5` 读出 `prev[slot]` 的 `len(+0x10)/ptr(+0x8)`，
  `malloc` 一块新 buffer，`memcpy` 后写进临时的 `{cap,ptr,len}`（`0x51b8cb/0x51b8d3/0x51b8db`）。
  个体结构 `+0x18..+0x30` 的 24 字节评分元组**一次都没读**。
- 紧接着无条件走评分块（`0x51b8fa jmp 0x51b961` → … → `0x51bc93 call 0x51a750`）。
- 整个 48 字节个体里没有布尔/标志字段（见 §2.2），也没有"dirty/valid"位被任何代码读。
- 亦未发现按"函数内容"缓存分数的结构：评分核 `0x51a750` 内 `call 0x47dec0`（哈希）3 次，都是**按士兵 id
  查位置/镜像**，不是按表达式查分数；轨迹是每次现算的（`0x51a93d call 0x4413b0`、`0x51a954 call 0x45fd30`）。

⇒ **游戏每代全量重评，我们的全量重评不比它慢**（反而少做：它每次评分还要多做 3 次哈希查找，
我们直接把 `x0/y0/镜像` 传进去）。

### 4.3 精评的真实语义（**不是**"更新分数"）

前 3 名用 0.01 再算一次，然后比较：
```asm
51c3b8: mov  eax,[rsp+0x108]      ; 新评分元组+0x10
51c3bf: cmp  eax,[rsp+0x38]       ; 旧元组+0x10（= 个体 +0x28）
51c3c3: jne  51c3ea               ; 不同 → 换掉
51c3c5: movsd xmm0,[rsp+0x100]    ; 新元组+0x08
51c3ce: subsd xmm0,[rsp+0x30]     ; 旧元组+0x08（= 个体 +0x20）
51c3d4: andpd xmm0,[abs]          ; |Δ|
51c3dc: ucomisd xmm0,[0x19f6d0]   ; 0.5
51c3e4: jbe  51c499               ; |Δ|<=0.5 → 接受（什么都不写）
```
- 接受分支 `0x51c499` **只释放临时 buffer**，不写分数 ⇒ 个体里留的仍是 0.5 粗评分。
- 拒绝分支 `0x51c3ea–0x51c451`：`makeRandomFunction + assemble + mutate`，然后
  `0x51c460–0x51c495` **只覆盖 cap/ptr/len**，`+0x18..+0x30` 原封不动 ⇒ **旧分数留着**。
  于是这一代返回的"第 r 名"可能是"一条随机函数 + 一个不属于它的分数"。
  （它会活到下一代：下一代先整体重评，所以影响只有"这一代交出去的答案"。）

### 4.4 障碍被击穿后旧分数会失效吗？

**会失效，而且游戏的处理方式是最粗暴的：不处理**——每代重评，分数自然跟着变；
没有脏标记、没有失效列表、没有增量重评。所谓"部分重评"在 A 里不存在（唯一的"部分"是
前 3 名的额外精评，而它只做校验）。

---

## 5. ③ 两个变体：什么时候用 B

### 5.1 `param_15` 的来源链（全部观测）

```
模块容器单例  FUN_0056e140(ctx)  → obj
              obj + 0x32 (u8)                     ← vaddr 0x479af0  movzx ecx,[rax+0x32]
variant = cl                                      ← vaddr 0x479af4  mov [rsp+0x3f],cl
   … 线程任务 payload 装配 …
payload+0x80 = [rsp+0x3f]                         ← vaddr 0x479d47  movzx edx,[rsp+0x3f]
                                                    vaddr 0x479d4c  mov [rsp+0x180],dl
payload+0x81 = r13b                                  vaddr 0x479d53  mov [rsp+0x181],r13b  ← 函数模式
   → FUN_00538d20(out, …, payload)   线程 spawn（含 RUST_MIN_STACK 解析）   vaddr 0x479da4 call 0x438d20
   → FUN_0053b3b0(job)               future/task body，0xB0 字节 job 整块拷贝
   → FUN_00538780(&cfg)              配置解包                        vaddr 0x438780
        a15 = *(u8*)(cfg+0x80)  =  variant     ← vaddr 0x4387bc  movzx ebp,[rdi+0x80]
        a16 = *(u8*)(cfg+0x81)  =  mode           vaddr 0x4387c3  movzx r15d,[rdi+0x81]
        （两者经 0x4387ec / 0x4387e7 作为第 15/16 实参压栈，0x438810 call 0x47a300）
   → FUN_0057a300(…, param_15, param_16, …)                        vaddr 0x47a300
        if (param_15 == 0) FUN_0061b440(…, 每槽个体数=cfg+0x78, &初始种群)   ← A
        else              FUN_0061d080(…, cfg+0x74)                        ← B
        （两条 call 在 vaddr 0x47a3d3 / 0x47a41e，与 docs/93 §2.3 一致）
```

要点：
- `param_15` 是**一个 bool 配置字节**，不是难度、不是回合、不是函数模式；
- 它同时被塞进 `FUN_0061eb90`（ctx 构造函数）的入参，所以 A/B 连 **ctx 可能都不一样**（未逐条核对，见 §9）；
- **A 比 B 多两个实参**：`每槽个体数`（`cfg+0x78`）和初始种群 Vec。**B 拿不到 `cfg+0x78`**
  ⇒ 变体 B 里"每槽 5 个个体 / 20% 精英"这套结构根本不存在；
- 反向：`cfg+0x80` 的值由**模块容器里的一个全局对象**决定，代码里**没有**按难度分支设置它的地方
  （难度只提供 `cfg+0x70/0x74/0x78` 那三个数）。

### 5.2 B（vaddr `0x51d080` / Ghidra `0x61d080`）到底在做什么

签名：`FUN_0061d080(out, ctx, p3, key, 代数, λ /*=cfg+0x74*/, &种群Vec)`。

```
base = 哈希表[ctx+0x18][key]                  // vaddr 0x4558c0（FUN_005558c0）取"这个士兵已有的函数"
if (base == 无) {                             // 哨兵 0x8000000000000000
    base = mutate(assemble(makeRandomFunction(mode)));   // 0x4ef020 + 0x4ee0e0 + 0x4ee720
}
if (mode == Normal) 一次特判处理              // vaddr 0x4eb830（FUN_005eb830），语义未核
t0 = Instant::now();
for (gen = 0; gen != 代数; gen++) {
    if (elapsed(t0) > 25s) break;              // 0x51d394–0x51d3ab（与 A 同构）
    base_f = fold_constants(base);             // 0x51d3c5 call 0x4ebbe0（每代循环头，紧跟 25 s 检查）
    pop = [];
    pop.push({base_f, score(base_f, 0.5)});    // 0x51d960
    for (i = 1; i != λ; i++) {
        if (i <= 20)      child = mutate(assemble(makeRandomFunction(mode)));  // 0x51df0c  ja/JA 门限 0x14
        else if (i <= 35) child = factory(mode, copy(base_f));                 // 0x51df79 门限 0x23
        else              child = factory(mode, copy(base_f));                 // 0x51dfc2（与上一支代码相同）
        pop.push({child, score(child, 0.5)});  // 0x51e213（评分前同样有 Normal 模式修正块）
    }
    sort_desc(pop);                            // >=21 → 0x4604c0，否则 0x41b720（与 A 不同实例，方向同为降序）
    for (item in pop) {                        // 排序后顺序扫描
        sc2 = score(item.f, 0.01);              // 0x51dcce
        if (sc2.+0x10 == item.+0x28 && |sc2.+0x08 − item.+0x20| <= 0.5) {
            base = item.f;  break;             // 0x51e2d7 接受：只保留这 1 个
        }
    }
    free(其余所有个体);
}
*out = vec![fold(base)];                       // 长度 1 的 Vec（0x51e54f call 0x4ebbe0 后再打包）
```

### 5.3 A ↔ B 行为差异（同一份配置下的对照）

| 维度 | A（`0x51b440`） | B（`0x51d080`） |
|---|---|---|
| 搜索范式 | **种群 GA**：s 个槽 × 5 个体 | **(1+λ) 爬山**：1 个基 + λ 个变体 |
| 每代个体数 | `5s` = 种群（10/20/30/40/50 的 5 倍 = 50…250） | `λ` = `cfg+0x74` = **10/20/30/40/50**（比 A 少 5 倍） |
| 父代 | 每个槽 1 个（第 i 名），4 个子代共用 | **每代只有 1 个基**，λ−1 个变体全从它出 |
| 是否用初始种群 Vec | **用**（第 1 代克隆它；实测为空） | **不用**（只 free 掉），种子来自哈希表 |
| 每代粗评 | `5s` | `λ` |
| 每代精评 | **固定 3** 次（前 3 名） | **最多 λ 次**（顺扫到第一个"通过校验"的就停） |
| 精评失败的处置 | 前 3 名中被判不合格的**留在种群**里（换成随机函数，旧分数保留），下一代替换 | 不合格就**继续扫下一条**；都没通过则基不动 |
| 排序+截断 | 排序后**截断到前 s**（=20%） | 排序后**只取 1 个**（不截断成 s） |
| 每代产出 | s 个存活个体 | 1 个基 |
| 出口 | 返回 **s 元 Vec** | 返回 **1 元 Vec** |
| 25 s 预算 | 有（`0x51b7cf`） | 有（`0x51d398`） |
| 精评一致性判据 | 同一个（`+0x28` 相等且 `|Δ+0x20| ≤ 0.5`） | 同一个 |

⇒ 一句话：**A 是"广撒网 + 每代砍到 20%"，B 是"一个函数每代造 λ 个变体，取第一个自洽的当新基"。**

### 5.4 运行时用的是哪个？

**A。** 证据（转引，§6）：`.rea-work/ga2.json` 的钩子按**返回地址落在 A 的函数体区间
`[0x51b440, 0x51cc60)`** 来打标，统计出 `SCORE` 2825 / `SIMPL`（0x4ebbe0）2772 / `FN_C`（工厂 0x4ee3a0）2217 次，
全部来自该区间；若走 B，这些返回地址应落在 `[0x51d080, 0x51e600)`，计数会是 0。
同一份数据里 `RAND/FN_A/FN_B` 各 30 次 ⇒ 只有第 1 代的 30 个槽走了随机重生分支（见 §6）。

---

## 6. 运行时交叉验证（转引，非本轮实验）

来源：`.rea-work/hook_ga2.js`（作者：另一个子 agent，2026-11-05）+ 它的输出 `.rea-work/ga2.json`
（20000 条事件上限，`c=1` 表示返回地址落在 A 的函数体内）。我**只做算术核对**，未重跑。

| 事件 | 含义 | 核心内计数 |
|---|---|---|
| `SCORE` | `call 0x51a750` | 2825 |
| `SIMPL` | `call 0x4ebbe0`（常量折叠，每个个体 1 次） | 2772 |
| `FN_C` | `call 0x4ee3a0`（工厂子代，每槽 4 次） | 2217 |
| `RAND`/`FN_A`/`FN_B` | `0x4ef020`/`0x4ee0e0`/`0x4ee720`（只有随机重生与"精评失败替换"会调） | 30 / 30 / 30 |

推断链条（**观测 + 算术**）：
1. 事件流前 60 条 = `RAND FN_A FN_B SIMPL SCORE` 然后 4 组 `SIMPL FN_C SCORE`，
   第 2 个 `RAND` 落在下标 17 ⇒ **每槽 17 个事件**、`RAND` 间距恒为 17、且 30 个 `RAND` 全在最前面
   ⇒ **第 1 代 30 个槽全部走随机重生**，其余各代不再有 `RAND` ⇒ **初始种群 Vec 长度为 0**、**s = 30**。
2. `s = 30` ⇒ 难度 3（代数 25、种群 150）；与 `.rea-work/aiargs_3.json` 记录的实参
   `(a4=25, a5=30, s0=5)` 完全一致（`a4`=代数、`a5`=s、`s0`=每槽个体数，正是 A 的实参形状）。
3. 用模型 `每代 SCORE=5s+3=153、SIMPL=5s=150、FN_C=4s=120` 拟合：
   `2825 = 153×18 + 71`、`2772 = 150×18 + 72`、`2217 = 120×18 + 57`
   ⇒ **18 个完整代 + 一个被窗口截断的第 19 代**，三个残差互相自洽（截断点落在同一位置附近）。
4. 完全没有 `SORT` 事件：`0x41b500` 只在 `len < 21` 时使用，本局 `5s = 150 ≥ 21` 走的是
   `0x4605f0`（那个 GOT 槽没被钩）⇒ 与静态分析一致。

⇒ 三项静态结论（每代 `5s+3` 次评分、工厂 `4s` 次、第 1 代全随机、父代 = 上一代同槽）
与这份运行时数据**逐项吻合**。

---

## 7. 我们实现还缺什么（`src/aigen.rs::evolve` 视角）

1. **父代模型要换**（①）：删掉 `a = next()%len` / `b = next()%len`；改成
   `for slot in 0..s { parent = survivors[slot].clone(); push(parent); for _ in 0..4 { push(factory(parent)) } }`。
   注意 4 个子代**共用同一个父代**，且父代是"上一代第 slot 名"。
2. **第 1 代**：初始种群为空 ⇒ 前 s 个个体是 `mutate(assemble(random_function(mode)))`，
   它们同样会各自产出 4 个工厂子代。（我们的实现若是"先随机初始化 5s 个"，就多了 `4s` 条不该有的谱系。）
3. **分数不缓存**（②）：保持每代全量重评 —— 游戏就是这样，我们不会更慢。
   但**不要**给个体加"分数有效位"，也不要跨代复用分数。
4. **精评语义**（②）：目前若把精评分写回分数，就与游戏不同。正确做法是
   "粗评分 → 排序 → 截断 → 前 3 名用 0.01 重算 → **只做校验**，不通过就把该条**整条换成
   `mutate(random_function)` 且保留旧分数**"。
   这一步会真实改变交给上层的那一发（`最优 = pop[0]`）。
5. **每代开销结构**（②）：`种群` 次粗评 + `4s` 次工厂 + `5s(+3)` 次折叠/求值。要与游戏的 25 s
   行为对齐，除评分外这些开销的比例也得对上（我们现在只有评分）。
6. **修正项的位置**：Normal 模式下，**每一个参与评分的个体**（含克隆本身、4 个子代、精评的 3 个）在评分前都要追加
   `+ const(y0 − f(x0))`（`0x51bbde`/`0x51bc21..0x51bc70`，精评 3 处也一样）；DiffEq 模式不追加。
   若我们只在"最终发射"时加修正，那么排序用的分数就与游戏不同。
7. **变体 B**：静态上确实存在且是另一套算法。**运行时只观测到 A**。
   建议：先把 selector 留在接口上（`use_variant_b: bool`），实现 A；B 标成"已知存在、未实现"，
   并在文档里写清"我们测到的每一局都是 A"（§6）。
8. **返回值的语义**：A 返回的是 **s 元 Vec**（不是 1 个函数）。上层再从里面挑（挑第 0 个 =
   最优；但要记住 §4.3 的"前 3 名可能被换成随机函数且带着旧分数"）。
9. `cfg+0x78 = 5`、`cfg+0x80` 这两个字段的**字面量来源仍未找到**（沿用 `docs/95` §9 的结论，
   本轮只确认了 `cfg+0x80` 的读取链，见 §5.1）。

---

## 8. 证据等级表

| # | 结论 | 依据（vaddr） | 等级 |
|---|---|---|---|
| 1 | 每代 = `s` 槽 × `param_7`(=5) 个体 = `5s` | `0x51b829`、`0x51bd5f`/`0x51c19c` | 观测 |
| 2 | 槽 i 的克隆源是上一代第 i 名（`prev[i]`，偏移 `i×48`） | `0x51b832`/`0x51b836`/`0x51b845`/`0x51b874` | 观测 |
| 3 | `prev` 就是"降序排序后截断到 s"的 Vec | `0x41b57c`（降序）、`0x51c203`/`0x51c208`、`0x51cb31–0x51cb41` | 观测 |
| 4 | 4 个工厂子代共用同一父代且没有第二父代 | `0x51bdd7`/`0x51be93`（`lea rdx,&parent`）+ `FUN_005ee3a0` 只读 `param_3` | 观测 |
| 5 | 随机重生只在"槽号 ≥ 上一代长度"时发生；实际只在第 1 代**且缓存为空**时（见 `docs/110`） | `0x51b829`/`0x51b82c` + §6 的 `RAND` 计数 | 观测 + 观测（转引） |
| 6 | 个体 48 字节、分数在 `+0x18`、克隆不复制分数 | §2.2 的逐字段证据 | 观测 |
| 7 | 每代 `5s` 粗评 + 3 精评（函数体内 `call 51a750` 恰 5 条） | `0x51bc93`/`0x51c0c9`/`0x51c3b3`/`0x51c6ee`/`0x51c9a8` | 观测 |
| 8 | 无分数缓存、无有效位；障碍变化靠每代重评兜住 | §4.2（克隆不含分数；无标志字段；评分核 3 次哈希都是按士兵 id） | 观测（有界否定：范围 = A/B 的驱动 + 评分核） |
| 9 | 精评只做校验、不写回；失败→整条换成随机函数且旧分数保留 | `0x51c3b8–0x51c3e4`、`0x51c3ea–0x51c495` | 观测 |
| 10 | `param_15` = `cfg+0x80`；`0→A`、`非0→B` | `0x4387xx`（`param_1+0x10`）、`0x47a3d3`/`0x47a41e` | 观测 |
| 11 | `param_15` 的源头 = 模块容器单例 `+0x32` 字节 | `0x479ac6 call 0x46e140`、`0x479af0 movzx ecx,[rax+0x32]`、`0x479af4`、`0x479d47/0x479d4c` | 观测 |
| 12 | B = (1+λ) 爬山、λ = `cfg+0x74`、无 `cfg+0x78`、返回 1 个个体 | `0x51d080` 伪代码 + `0x51d960`/`0x51e213`/`0x51dcce`/`0x51e54f` | 观测 |
| 13 | 运行时实际走 A | `.rea-work/ga2.json`（按 A 的区间过滤仍有 2825 次 SCORE） | 观测（转引） |
| 14 | 每代 `5s+3` 与运行时计数吻合（s=30、18 代 + 截断） | 同上，算术 | 观测（转引）+ 算术 |
| 15 | 工厂"取一个完整子表达式"而非任意字节区间 | `FUN_004be7d0`(vaddr `0x3be7d0`) 的递归取操作数逻辑 | 观测（结构）/ 推断（措辞） |
| 16 | `cfg+0x80` 的语义名（哪个玩法开关） | 只追到"模块容器某对象的 `+0x32`"，类型名未解析 | **未知** |
| 17 | 变体 B 的 `<21 / <36` 两段门限为什么这么分（两段代码相同） | 未解释（可能是编译器复制/不同容量的 append 路径） | **推断（未证实）** |

---

## 9. 仍未确定

1. **`cfg+0x80` 的语义名**：追到了"模块容器里某个共享对象的 `+0x32` 字节"（`FUN_0056e140`），
   但该对象的类型名是模块容器里一个 16 字节常量做 key 查出来的，本轮没解析它
（⚠️ **地址空间更正**：本节原来写的 `0x29dab0` 是 **Ghidra 地址**，虚地址是 **`0x19dab0`**；
按虚地址去读 `0x29dab0` 会读到 Godot 方法名字符串，完全无关。另：配置里的 `+0x32` 是
**相对 ArcInner** 的偏移，模块结构体内是 `+0x22`，结构体 size `0x28`）
   ⇒ 到底哪个玩法/设置开关会把它置 1，**未知**。
2. **`param_15` 也被传进 `FUN_0061eb90`（ctx 构造）**，A/B 的 ctx 是否有别（例如 `limit` 或哈希表内容不同）
   未逐条核对。
3. **B 的两段门限**（`i<=20` / `i<=35`）为什么会存在：两段代码几乎相同（都 `copy(base) → factory`），
   可能是编译器为不同容量路径复制的两份；未证实。
4. **B 的 `FUN_004eb830`**（`if (mode==Normal)` 对种子做一次处理）语义未核。
5. **`cfg+0x68`**（A 的第 3 实参）在 A 里**完全没被使用**（死参数）——但它被一路传下来，
   说明上游还有别的分支在用；本轮未追。
6. **`0x4605f0`/`0x4604c0` 两个归并排序实例**只核到"由 GOT 槽 `0x9bbdc0`/`0x9bbdb8` 解析而来"，
   内核方向沿用 `docs/95` 的**推断（降序）**；A 的插入排序版是直接读死的降序（`0x41b57c`），
   B 的版同样直接读死（`0x41b79c`），两个"<21 元素"版本本轮都亲手核过。
7. 运行时数据只有**一局、难度 3、18.5 代**的样本；A 的其它档位、以及"精评失败替换真的发生时
   种群会怎样"没有运行时观测。

---

## 附录 A · 复现（全部只读）

```bash
cd /home/user/MyProject/gw2

# 1) A 的一代：槽循环 + 克隆 + 随机重生分支
objdump -d -M intel --start-address=0x51b7c0 --stop-address=0x51b970 libgraphwarrust.so
# 2) 工厂子代的父代搬运（父代 = 本槽 cur 的拷贝）
objdump -d -M intel --start-address=0x51bdb2 --stop-address=0x51bea0 libgraphwarrust.so
# 3) 粗评 / 精评的 5 个调用点
objdump -d -M intel --start-address=0x51b440 --stop-address=0x51d080 libgraphwarrust.so | grep 'call *51a750'
# 4) 排序方向（插入排序版，直接读死）
objdump -d -M intel --start-address=0x41b560 --stop-address=0x41b590 libgraphwarrust.so
# 5) 截断到 s
objdump -d -M intel --start-address=0x51c1f0 --stop-address=0x51c220 libgraphwarrust.so
# 6) 精评一致性判定 + 替换后只写 cap/ptr/len
objdump -d -M intel --start-address=0x51c3b3 --stop-address=0x51c4a0 libgraphwarrust.so
# 7) param_15 的源头
objdump -d -M intel --start-address=0x479ac0 --stop-address=0x479b00 libgraphwarrust.so
objdump -d -M intel --start-address=0x479d40 --stop-address=0x479db0 libgraphwarrust.so
# 8) 变体 B 的两段门限与工厂调用
objdump -d -M intel --start-address=0x51df00 --stop-address=0x51e030 libgraphwarrust.so
```

REA 侧（Ghidra 地址 = vaddr + 0x100000，同一 session 内）：

```
procedure_pseudo_code 0x61b440     # A
procedure_pseudo_code 0x61d080     # B
procedure_pseudo_code 0x57a300     # A/B 分派器
procedure_pseudo_code 0x538780     # 配置解包（读 cfg+0x80/0x81）
procedure_pseudo_code 0x579520     # AI 回合体：取 variant、装 payload、spawn
procedure_pseudo_code 0x56e140     # 模块容器取单例（variant 的真正源头）
procedure_pseudo_code 0x5ee3a0     # 子代工厂
procedure_pseudo_code 0x4be7d0     # 取一个完整子表达式
```

## 附录 B · 本轮用到的 REA 证据 id

| 证据 | 内容 |
|---|---|
| `ev_545e1ee15ddc9279b1b0df7d77faff4cc3e9b8b1b0577eac1b714d14fc73e840` | `FUN_0061b440` 全函数伪代码（A；本轮的父代/评分/截断/精评四处结论的底稿） |
| `ev_38637fc7683cf3468be5ea7021bb3ed5cc76984fba66bcab87c400dcb062ec3d` | `FUN_0061d080` 全函数伪代码（B） |
| `ev_d3101214c22bb83667457aae49e70d3b1b5c2483782d40736f52ce77105b99be` | `FUN_0057a300` A/B 分派器伪代码 |
| `ev_a2bf912a2d5ada2b102b8efa6217dca923e443d892ee949b12cdbb39e4896f25` | `FUN_00538780` 配置解包（`param_1+0x10` = `cfg+0x80`） |
| `ev_eb9a7fa687408f3a872f39c21941586d0446c90e9c6ec26564acbf493762711d` | `FUN_00579520` AI 回合体（variant 装配、payload、spawn） |
| `ev_dc5969b983f7ad71120c21f8d38267d77a97c397f9c7139942b8512b1334dd22` | `FUN_0056e140` 模块容器取单例（variant 源头） |
| `ev_4c6b65f526a7d3ba6abce5a691a01ecc7ee2617cfa097d4dd683f697c4d9d57e` | `FUN_005ee3a0` 子代工厂伪代码 |
| `ev_32c21edef42312f39cf491c40116dc21918c2c5291f5f41eac7be404831ecb80` | `FUN_004be7d0` 取完整子表达式 |
| `ev_0ee9b5a15b24164e50b0041f44f215962bd3b9a8c067b4214d71f6babf1bee60` | `FUN_00538d20` 线程 spawn（payload 0x88 字节，`RUST_MIN_STACK`） |
| `ev_eb7e3d9380ca372ea6508acf8f2a2dc9acda2c7bf76d9692a242b0023e583b97` | `FUN_0053b3b0` 任务体（0xB0 job 整块拷贝后调 `FUN_00538780`） |
| `ev_5bac5c4c8f6fc287151e6a8b9756378fe496b87e03ed4cd16fd8a83e76fe1915` | `FUN_005ebbe0` 常量折叠（本轮的 `SIMPL` 事件） |
| `ev_f105a3b2c06ff10a1ac3dd2059dd0a5be7779ebed4dac46db34245dcc7b0ecc1` | （本轮首次误查 `0x67a300` 的产物，仅说明地址空间陷阱，未用于结论） |
