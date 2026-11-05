# 91 · normal 模式二分循环的逐条汇编还原（`lo` / `hi` 更新规则）

状态：现行
上游：`docs/89` §26–29（缺陷定位到 `graphwar_curve.py` 的 `lo = x` 一行）
结论：`docs/89` 说对了「`lo = x`」这一半，**漏了同一分支里的 `hi = mid2`**。
补上之后，真二进制在 24 个案例上**逐点一致**（含 2001 点长跑、两种 flip、三种步长）。

---

## 0. 一句话结论

> **半步复检不通过（`chord <= chord2`）时，`lo` 回到基点 `x`，同时 `hi` 被拉到 `mid2 = (x+mid)/2`。**
> 参考实现只做了前者，`hi` 保持原值 ⇒ 拒绝后的搜索区间宽了一倍 ⇒ 从第 4 点起走到另一条轨迹。

两条 `mov` 就写在比较指令的两侧，位置极容易被漏读：

| 地址 | 指令 | 作用 |
|---|---|---|
| `0x3c2a0a` | `movsd %xmm0,0x50(%rsp)` | **`hi = mid2`**（`0x50(%rsp)` = `hi`，在比较**之前**无条件写） |
| `0x3c2a47` | `movsd 0x70(%rsp),%xmm0` | **`lo = x`**（`0x70(%rsp)` = 基点 `x`，在 `jbe` **之前**装好） |
| `0x3c2a4d` | `jbe 0x3c2955` | `chord <= chord2` → 带着新的 `(lo=x, hi=mid2)` 进下一轮 |

证据等级：**逐位验证（读指令）+ 逐点对照真二进制（24/24）**。

---

## 1. 方法、地址空间、可复现命令

| 项 | 值 |
|---|---|
| 二进制 | `libgraphwarrust.so`，sha256 `9a53811518e7…bed187` |
| 采样器 | ELF vaddr `0x3c2530`（Ghidra `0x4c2530`） |
| 本文范围 | vaddr `0x3c2923`（normal 分支入口）– `0x3c2b57`（外层次数回边） |
| 反汇编 | `objdump -d --no-show-raw-insn --start-address=0x3c2530 --stop-address=0x3c2b5d libgraphwarrust.so`（从函数入口起，避免半条指令） |
| 交叉核对 | REA/Ghidra 12.1.4 `inspect_native_instruction`（独立解码器，字节一致，见 §8.3） |
| oracle | `analysis/gwemu.py` 的 `Gw.call_path`（unicorn 直接跑真二进制） |

**注意**：`objdump --start-address=0x3c29a0` 会从**半条指令**开始（`3c29a0` 不是指令边界，
会解出 `sub %al,(%rsp)` 这种垃圾）。必须从 `0x3c2530`（函数入口）或已确认的边界
（`0x3c2923` / `0x3c295d`）起解。本文所有地址都已用「从 `0x3c2530` 线性扫下来」验证过。

---

## 2. 变量地图（全部来自指令，逐位验证）

函数签名（System V，从 `0x3c2541`–`0x3c255a` 的入栈顺序读出）：

```
rdi = *out（Vec<(f64,f64)> 的返回槽）
rsi = token 向量指针                 -> r14
edx = flip  标志（低字节 0x6c(%rsp)） -> 入口存 ebp，随后 ebp 改作点数计数器
ecx = ode   标志（低字节 r15b）       -> r15d
xmm0 = x0    -> (%rsp) 入口 / 随后 0x70(%rsp)
xmm1 = y0    -> 0x20(%rsp)
xmm2 = step  -> 0x60(%rsp)
```

| 槽 / 寄存器 | 含义 | 写入点 |
|---|---|---|
| `(%rsp)` | `mid`（本轮候选，也是接受后的新 `x`） | `0x3c2988` |
| `0x18(%rsp)` | 求值器的临时 token 槽（每次调用前清 0） | `0x3c2974`、`0x3c29f6` |
| `0x20(%rsp)` | **当前点 `y`** | 入口 `0x3c254f`；接受时 `0x3c2af4` |
| `0x38/0x40/0x48(%rsp)` | 结果 Vec 的 `cap / ptr / len` | `0x3c255d`–`0x3c2576` |
| `0x50(%rsp)` | **`hi`**（复检段临时兼作 `mid2`） | 初值 `0x3c2939`；`0x3c2a0a`；`0x3c2a86` |
| `0x60(%rsp)` | **`step`**（= 0.5 或 0.01） | `0x3c2544` |
| `0x6c(%rsp)` | **`flip` 标志** | `0x3c25af` |
| `0x70(%rsp)` | **当前点 `x`**（弦的起点；`lo = x` 的来源），每个点重置一次 | `0x3c2944`；接受时 `0x3c2ac4` |
| `0x90(%rsp)` | **`lo` 的暂存**（内层每轮开头写入）；复检段临时改存 `chord` | `0x3c295d`、`0x3c2a13` |
| `0xa0(%rsp)` | **`f(mid)`** | `0x3c2995` |
| `0xb0(%rsp)` | `*out` 指针 | `0x3c25a7` |
| `xmm0` | **`lo` 的活值**（跨迭代靠它传，`0x90` 只是备份） | 各分支 |
| `xmm3` | `chord` 的暂存 | `0x3c29bb` |
| `ebx` | **二分计数器**，初值 **`0x64` = 100** | `0x3c293f`，`dec` 在 `0x3c2955` |
| `ebp` | 已接受点数（入口时曾短暂存放 `flip` 实参 `edx`，`0x3c25cb` 清零后复用） | `0x3c2b23` `inc` |
| `r13d` | 点数预算 `(200/step) as i32`，`max(·,1)` | `0x3c25bd`–`0x3c25f1`（`cvttsd2si` 在 `0x3c25e1`） |
| `r12` | **`hypot` 的函数指针**（GOT `0x9baca8`） | `0x3c2928` |
| `r14` | token 向量 | `0x3c254c` |
| `r15` | `&0x18(%rsp)`（求值器第 2 实参） | `0x3c2923` |

**关键点**：`0x50(%rsp)` 同时是「`hi`」和「`mid2` 的暂存」。复检段把 `mid2` 写进去之后
**没有恢复**，直接进入比较 —— 于是「`mid2` 留在 `hi` 里」就是本轮的结果。

求值器调用约定（`0x3c2985`–`0x3c2990`）：`rdi=tokens, rsi=&tmp, xmm0=mid,
xmm1=0.0 (y), xmm2=0.0 (y')`。normal 模式 `y` 与 `y'` 都传 0，与
`graphwar_curve.py` 的 `f(mid) = ev.eval(mid, 0.0, 0.0)` 一致。

---

## 3. 逐条指令清单（`0x3c2923`–`0x3c2b57`，带注释）

标注含义：`【逐位】`= 直接读指令得到，无推断；`【推断】`= 需附说明缺什么。

```
; ─────────── 外层循环入口：为「下一个点」准备二分区间 [x, x+step] ───────────
3c2923: lea 0x18(%rsp),%r15        ; r15 = &tmp（求值器 2nd 参）
3c2928: mov 0x5f8379(%rip),%r12    ; r12 = hypot 指针（GOT 0x9baca8）
3c292f: movsd 0x60(%rsp),%xmm0     ; xmm0 = step                ← 外层回边目标
3c2935: addsd %xmm1,%xmm0          ; xmm0 = step + x           ; xmm1 = x（基点）
3c2939: movsd %xmm0,0x50(%rsp)     ; hi  = x + step            【逐位】
3c293f: mov $0x64,%ebx             ; ebx = 100  ← MAX_BISECT   【逐位】
3c2944: movsd %xmm1,0x70(%rsp)     ; x   = xmm1（本点的基点）   【逐位】
3c294a: movapd %xmm1,%xmm0         ; lo  = x                   【逐位】
3c294e: jmp 3c295d                 ; 进入二分循环（第一次不 dec！）

; ─────────── 内层循环的「计数 + 回边」小段 ───────────
3c2950: movapd (%rsp),%xmm0        ; lo = mid   ← 弦太短时走这里  【逐位】
3c2955: dec %ebx                   ; 计数器--                  【逐位】
3c2957: je 3c2b5d                  ; 100 次用完 → 整个函数返回  【逐位】
3c295d: movsd %xmm0,0x90(%rsp)     ; [0x90] = lo（备份）  ← 内层循环头  【逐位】
3c2966: addsd 0x50(%rsp),%xmm0     ; xmm0 = lo + hi
3c296c: mulsd [0x19f6d0],%xmm0     ; xmm0 = (lo+hi)*0.5  = mid   (0x19f6d0 = 0.5)
3c2974: movq $0x0,0x18(%rsp)       ; 清临时 token 槽
3c297d: xorpd %xmm1,%xmm1          ; y  = 0
3c2981: xorpd %xmm2,%xmm2          ; y' = 0（死参数，全二进制 7 处都一样）
3c2985: mov %r14,%rdi              ; rdi = tokens
3c2988: movapd %xmm0,(%rsp)        ; mid = (lo+hi)*0.5           【逐位】
3c298d: mov %r15,%rsi
3c2990: call 4eb9d0                ; ymid = f(mid)   （求值器）
3c2995: movapd %xmm0,0xa0(%rsp)    ; [0xa0] = f(mid)
3c299e: movapd (%rsp),%xmm0        ; xmm0 = mid
3c29a3: subsd 0x70(%rsp),%xmm0     ; xmm0 = mid - x              【逐位】弦用基点 x
3c29a9: movapd 0xa0(%rsp),%xmm1    ; xmm1 = f(mid)
3c29b2: subsd 0x20(%rsp),%xmm1     ; xmm1 = f(mid) - y
3c29b8: call *%r12                 ; chord = hypot(mid-x, f(mid)-y)   【逐位】
3c29bb: movapd %xmm0,%xmm3         ; xmm3 = chord（存住，复检段要用）
3c29bf: movsd 0x60(%rsp),%xmm1     ; xmm1 = step
3c29c5: subsd %xmm1,%xmm0          ; xmm0 = chord - step
3c29c9: andpd [0x19c050],%xmm0     ; xmm0 = |chord - step|  (0x19c050 = 0x7fff…ff 掩码)
3c29d1: movsd [0x19fc78],%xmm2     ; xmm2 = tol = 0.001
3c29d9: ucomisd %xmm0,%xmm2        ; 比较 tol 与 err
3c29dd: jbe 3c2a60                 ; tol <= err（含 NaN）→ 「不满足」分支  【逐位】
                                   ; ⇒ 判据是严格 |chord-step| < tol，等号算不满足

; ─────────── 容差满足：半步复检（mid2 = 基点与 mid 的中点） ───────────
3c29e3: movsd 0x70(%rsp),%xmm0     ; xmm0 = x（基点，不是 lo！）  【逐位】
3c29e9: addsd (%rsp),%xmm0         ; xmm0 = x + mid
3c29ee: mulsd [0x19f6d0],%xmm0     ; xmm0 = (x + mid) * 0.5 = mid2  【逐位】
3c29f6: movq $0x0,0x18(%rsp)
3c29ff: xorpd %xmm1,%xmm1          ; y  = 0
3c2a03: xorpd %xmm2,%xmm2          ; y' = 0
3c2a07: mov %r14,%rdi
3c2a0a: movsd %xmm0,0x50(%rsp)     ; ★ hi = mid2（无条件，先写后判）  【逐位】
3c2a10: mov %r15,%rsi
3c2a13: movapd %xmm3,0x90(%rsp)    ; [0x90] = chord（覆盖 lo 的备份，见 §9.2）
3c2a1c: call 4eb9d0                ; f(mid2)
3c2a21: movapd %xmm0,%xmm1         ; xmm1 = f(mid2)
3c2a25: movsd 0x50(%rsp),%xmm0     ; xmm0 = mid2
3c2a2b: subsd 0x70(%rsp),%xmm0     ; xmm0 = mid2 - x
3c2a31: subsd 0x20(%rsp),%xmm1     ; xmm1 = f(mid2) - y
3c2a37: call *%r12                 ; chord2 = hypot(mid2-x, f(mid2)-y)
3c2a3a: movapd 0x90(%rsp),%xmm1    ; xmm1 = chord（取回）
3c2a43: ucomisd %xmm0,%xmm1        ; 比较 chord（xmm1）与 chord2（xmm0）  【逐位】
3c2a47: movsd 0x70(%rsp),%xmm0     ; ★ xmm0 = x（为「退回」预装 lo=x；不改标志）  【逐位】
3c2a4d: jbe 3c2955                 ; chord <= chord2（含 NaN）→ 退回  【逐位】
3c2a53: jmp 3c2aa0                 ; chord >  chord2          → 接受

; ─────────── 容差「不满足」：普通二分 ───────────
3c2a60: movapd %xmm1,%xmm0         ; xmm0 = step
3c2a64: cmpltsd %xmm3,%xmm0        ; xmm0 = (step < chord) ? 全1 : 0
3c2a69: xorpd %xmm1,%xmm1          ; xmm1 = 0
3c2a6d: cmpunordsd %xmm1,%xmm3     ; xmm3 = isnan(chord) ? 全1 : 0
3c2a72: orpd %xmm0,%xmm3           ; = (step < chord) || isnan(chord)   【逐位】
3c2a76: movd %xmm3,%eax
3c2a7a: test $0x1,%al
3c2a7c: je 3c2950                  ; 假 ⇒ chord <= step ⇒ lo = mid      【逐位】
3c2a82: movaps (%rsp),%xmm0        ; 真 ⇒ 走下面三行
3c2a86: movsd %xmm0,0x50(%rsp)     ; hi = mid                            【逐位】
3c2a8c: movsd 0x90(%rsp),%xmm0     ; lo 不变（取回备份）                  【逐位】
3c2a95: jmp 3c2955                 ; → dec / 继续

; ─────────── 接受：符号、NaN、边界、入列 ───────────
3c2aa0: cmpb $0x0,0x6c(%rsp)       ; flip == 0 ?
3c2aa5: jne 3c2aae
3c2aa7: movapd (%rsp),%xmm1        ; xmm1 = mid（不翻转）
3c2aac: jmp 3c2abb
3c2aae: movapd (%rsp),%xmm1
3c2ab3: xorpd [0x19e690],%xmm1     ; flip 时取 -mid（0x19e690 = 符号位掩码）
3c2abb: movapd 0xa0(%rsp),%xmm0    ; xmm0 = f(mid)
3c2ac4: movapd %xmm1,0x70(%rsp)    ; [0x70] = ±mid（输出用的 x；下轮会被 0x3c2944 覆盖成裸 mid）
3c2aca: ucomisd %xmm1,%xmm0        ; 一条指令同时测 f(mid) 与 ±mid 的 NaN
3c2ace: jp 3c2b5d                  ; 任一为 NaN → 整条曲线结束            【逐位】
3c2ad4: movapd (%rsp),%xmm0
3c2ad9: andpd [0x19c050],%xmm0     ; |mid|
3c2ae1: ucomisd [0x19fbc8],%xmm0   ; 与 25.0 比
3c2ae9: ja 3c2b5d                  ; |mid| > 25 → 结束                     【逐位】
3c2aeb: movapd 0xa0(%rsp),%xmm0    ; xmm0 = f(mid)
3c2af4: movapd %xmm0,0x20(%rsp)    ; y = f(mid)（给下一点用）
3c2afa: andpd [0x19c050],%xmm0     ; |y|
3c2b02: ucomisd [0x19fd40],%xmm0   ; 与 15.0 比
3c2b0a: ja 3c2b5d                  ; |y| > 15 → 结束                       【逐位】
3c2b0c: mov 0x48(%rsp),%rbx        ; Vec 长度
3c2b11: cmp 0x38(%rsp),%rbx        ; 与 cap 比
3c2b16: jne 3c2b23
3c2b18: lea 0x38(%rsp),%rdi
3c2b1d: call *…                    ; 容量不足则 grow
3c2b23: inc %ebp                   ; 点数计数器++
3c2b25: mov 0x40(%rsp),%rax        ; Vec ptr
3c2b2a: mov %rbx,%rcx
3c2b2d: shl $0x4,%rcx              ; *16（每点 16 字节）
3c2b31: movaps 0x70(%rsp),%xmm0    ; ±mid
3c2b36: movsd %xmm0,(%rax,%rcx,1)  ; pts[i].x
3c2b3b: movapd 0x20(%rsp),%xmm0    ; y
3c2b41: movsd %xmm0,0x8(%rax,%rcx,1) ; pts[i].y
3c2b47: inc %rbx
3c2b4a: mov %rbx,0x48(%rsp)        ; 更新 Vec 长度
3c2b4f: cmp %r13d,%ebp             ; 点数 == 预算 ?
3c2b52: movapd (%rsp),%xmm1        ; x = 裸 mid（下一点的基点；注意不带 flip）
3c2b57: jne 3c292f                 ; ← 外层回边                          【逐位】
3c2b5d: …（把 Vec 写回 *out，ret）
```

常量已按 `HANDOVER.md` §3 的「`.rodata` 文件偏移 == 虚地址」逐个读出核对：
`0x19f6d0 = 0.5`、`0x19fc78 = 0.001`、`0x19fbc8 = 25.0`、`0x19fd40 = 15.0`、
`0x19c050 = 0x7FFFFFFFFFFFFFFF`、`0x19e690 = 0x8000000000000000`、
`0x19f8c0 = 200.0`、`0x19f098 = 6.0`（RK4 的 `/6`，属 ODE 分支）。

---

## 4. 五问五答

### 4.1 `chord` 与 `chord2` 比较之后，两个分支各跳哪里？

`0x3c2a43: ucomisd %xmm0,%xmm1` —— AT&T 逆序显示，实际是 `chord(XMM1)` vs `chord2(XMM0)`。
`ucomisd dest=xmm1`，故标志位 = `xmm1 ? xmm0` = `chord ? chord2`。

| 条件 | 目标 | 语义 |
|---|---|---|
| `jbe` 成立（`chord <= chord2`，**含 unordered/NaN**） | `0x3c2955` → `0x3c295d` | **复检不通过 ⇒ 退回** |
| `jbe` 不成立（`chord > chord2`） | `0x3c2aa0` | **复检通过 ⇒ 接受 `mid`** |

【逐位，读指令；Ghidra 独立解码亦为 `JBE 0x004c2955`，见 §8.3】

### 4.2 复检「通过」时 `lo` / `hi` 写成什么？

**什么都不写**。`0x3c2a53: jmp 0x3c2aa0` 直接进接受段，`mid` 成为新点、成为下一轮的基点
（`0x3c2b52`），`lo`/`hi` 下一轮由 `0x3c2939`/`0x3c2944` 重新初始化。
【逐位】

### 4.3 复检「不通过」时跳哪、`lo`/`hi` 怎么变？ ← **核心**

```
0x3c2a0a  hi  := mid2 = (x + mid) * 0.5      ; 早在比较之前就写了
0x3c2a47  lo  := x                            ; 预装进 xmm0
0x3c2a4d  jbe 0x3c2955                        ; → dec ebx → 0x3c295d
0x3c295d  [0x90] := lo ; xmm0 := lo + hi = x + mid2
```

即：

```
lo = x
hi = (x + mid) / 2
```

**注意 `hi` 用的是 `mid2`，不是 `mid`。** 这是与参考实现唯一的分歧点。
【逐位：`0x3c2a0a` 确认落点是 `0x50(%rsp)`；`0x3c2966` 确认 `0x50(%rsp)` 参与 `lo+hi`】

副作用（重要）：这次「退回」**不是**在原有区间内收缩，而是把区间重置成 `[x, (x+mid)/2]`。
当 `lo` 已经被之前的迭代推到 `x` 右边时，新区间可能**与旧区间不交**、甚至更宽
（例：`lo=x+0.4s, hi=x+0.45s` → 新区间 `[x, x+0.2125s]`）。
所以这个二分**不是单调收敛**的，`MAX_BISECT=100` 不是多余保险。
【逐位（由 4.1–4.3 的更新式直接推出）+ 实测（§6 的轨迹确实跳回去了）】

### 4.4 循环回边在哪？计数器（MAX_BISECT=100）在哪个寄存器？

| 环 | 回边 | 次数控制 |
|---|---|---|
| **内层（二分）** | `0x3c2a4d` / `0x3c2a95` / `0x3c2a7c`（→ `0x3c2950`）三条都汇到 `0x3c2955`，再落到循环头 `0x3c295d` | **`ebx`**，`0x3c293f: mov $0x64,%ebx`（=100），`0x3c2955: dec %ebx`，`0x3c2957: je 0x3c2b5d` |
| **外层（每点一次）** | `0x3c2b57: jne 0x3c292f` | `ebp`（已接受点数）vs `r13d`（预算 `(200/step) as i32`，`max(·,1)`），`0x3c2b4f: cmp %r13d,%ebp` |

**首次进入不 `dec`**：`0x3c294e: jmp 0x3c295d` 绕过 `0x3c2955`，所以第 1 轮不算次数，
总迭代上限就是 100。
计数用尽（`je 0x3c2b5d`）时**直接返回已有点**（不裁剪、不兜底），这就是 `docs/89` §9 的「提前中止」。
【逐位】

### 4.5 整段 normal 分支的等价伪代码

见 §5。

---

## 5. 等价伪代码

```python
# 二分：求 mid ∈ [x, x+step]，使 hypot(mid-x, f(mid)-y) 尽量接近 step
lo, hi = x, x + step
for _ in range(100):                       # 0x3c293f  ebx = 100
    mid = (lo + hi) * 0.5                  # 0x3c296c / 0x3c2988
    fm  = f(mid)                           # 0x3c2990
    chord = hypot(mid - x, fm - y)         # 0x3c29b8（注意用 x，不是 lo）

    if not (0.001 > abs(chord - step)):    # 0x3c29d9 ucomisd + 0x3c29dd jbe
        if chord > step or isnan(chord):   # 0x3c2a64/0x3c2a6d/0x3c2a72
            hi = mid                       # 0x3c2a86
        else:
            lo = mid                       # 0x3c2950
        continue

    # 半步复检
    mid2   = (x + mid) * 0.5               # 0x3c29ee（用 x，不是 lo）
    fm2    = f(mid2)                       # 0x3c2a1c
    chord2 = hypot(mid2 - x, fm2 - y)      # 0x3c2a37
    hi = mid2                              # 0x3c2a0a  ★ 无条件写
    if not (chord > chord2):               # 0x3c2a43 ucomisd + 0x3c2a4d jbe
        lo = x                             # 0x3c2a47   ★ 退回基点
        continue                           # ★★ 二者都要，缺一就像参考实现那样跑偏

    # 接受
    px = -mid if flip else mid             # 0x3c2aa0–0x3c2ab3
    if isnan(fm) or isnan(px): return pts  # 0x3c2aca jp
    if abs(mid) > 25.0:        return pts  # 0x3c2ae1 ja
    if abs(fm)  > 15.0:        return pts  # 0x3c2b02 ja
    pts.append((px, fm))
    x, y = mid, fm
    break                                  # 出内层，算下一个点
else:
    return pts                             # 100 次未收敛：整条曲线到此为止
```

要点（三处「用 `x` 不用 `lo`」）：弦长算 `mid - x`、`mid2` 算 `(x + mid)/2`、
退回时 `lo = x`。`x` 是**本点的基点**（`0x70(%rsp)`，整段内层不变），
`lo` 只是本轮搜索的下界。

⚠️ `docs/04` §2 那份伪代码是**本文之前**的版本：它只写「`|L-step|<0.001` 就接受」，
**缺了半步复检**（`mid2`/`chord2`），因此也缺了 `hi = mid2`。以本文 §5 为准。

ODE 模式**没有**这段复检（`0x3c2840` 在容差满足时直接接受），
所以 `path.rs`/`graphwar_curve.py` 的 `ode` 分支无需改动。【逐位：`0x3c27ee ja 3c2840` 后无 mid2】

---

## 6. 这就是第 4 点的真相（实测轨迹）

`docs/89` 的 case：`(abs(cos((-1.43*x)))*8.65) + 4.635142442441488`，
起点 `(-1.3401100016801577, 7.565109751327434)`，`step=0.5`，`flip=false`。
第 3→4 点这一段二分（`base` = 现行参考实现，`hi2` = 本文结论）：

```
it= 8 lo=-0.947531876680158 hi=-0.945578751680158 mid=-0.946555314180158 chord=0.5052760
it= 9 lo=-0.947531876680158 hi=-0.946555314180158 mid=-0.947043595430158 chord=0.5000276  err=2.8e-05  ← 容差满足！
        base: 复检不通过 → lo=x=-1.215110001680158, hi 保持 -0.946555314180158   区间宽 0.2686
        hi2 : 复检不通过 → lo=x=-1.215110001680158, hi = mid2 = -1.081076798555158 区间宽 0.1341
it=10 base mid=-1.080832657930158 / hi2 mid=-1.148093400117658          ← 从这儿开始分道扬镳
...   （两边都是 20 次迭代，不是「少走一次」，而是「退回后的上界不同」）
base 接受 mid=-1.174459633943341  chord=0.499648215115109  ← docs/89 §28 的参考实现值
hi2  接受 mid=-1.174402651902936  chord=0.500351212189173  ← 真二进制值（逐位相同）
```

⇒ `docs/89` §17/§18 里「差一次二分迭代」「接受时机晚一步」的说法**不对**。
真相是：**同一次退回，退回后的上界 `hi` 不同**，此后整段二分走向变了；
迭代次数恰好都是 20，点数却从 70 变成 401。

---

## 7. 修法（精确到行）

### 7.1 父仓库 `graphwar_curve.py`（normal 分支，第 471–473 行）

```python
                if ln <= ln2:          # 471
                    lo = x             # 472
                    continue           # 473
```

改成：

```python
                if not (ln > ln2):     # 471  ← 与 ucomisd+jbe 逐位等价（NaN 走退回）
                    lo = x             # 472
                    hi = mid2          # ★ 新增：0x3c2a0a
                    continue           # 473
```

- `hi = mid2` 的依据：`0x3c2a0a: movsd %xmm0,0x50(%rsp)`，其中 `xmm0 = (x+mid)*0.5`（`0x3c29ee`），
  而 `0x50(%rsp)` 正是 `0x3c2966` 的 `lo + hi` 里的 `hi`。
- 把 `ln <= ln2` 改成 `not (ln > ln2)` 的依据：`0x3c2a43 ucomisd` 后是 `jbe`，
  而 `ucomisd` 遇 NaN 会置 `ZF=CF=1` ⇒ `jbe` 成立 ⇒ 走退回。
  Python 的 `ln <= ln2` 在 `ln2` 为 NaN 时得 `False` ⇒ 会走接受。**这是唯一的第二处差异**，
  属 §9.1 的边界，**未实测**（标【推断】）。

### 7.2 `gw2-rebuild/src/path.rs`（第 92–95 行）

```rust
                if chord <= chord2 {   // 92
                    lo = x;            // 93
                    continue;          // 94
                }                      // 95
```

改成：

```rust
                if !(chord > chord2) {
                    lo = x;
                    hi = mid2;         // ★ 新增
                    continue;
                }
```

`hi` 在 `54` 行已是 `let mut hi`，`mid2` 在 `89` 行已算好，无需其它改动。
`Mode::Ode` 分支不经过这段，**不要**动。

### 7.3 验收标准（照 `docs/89` §11 的 oracle 数字，全部已跑通）

| 步长 | 点数 | 末点 |
|---|---|---|
| 0.5 | **401** | **(24.8098, 9.8719)** |
| 0.01 | **20001** | **(23.2128, 6.4176)** |

第 4 点必须 = `-1.174402651902936`。
改完还必须跑父仓库 `analysis/difftest.py` + `analysis/fuzz.py` 确认没有退化。
**已由改码一方跑完**（`docs/89` §32 记录）：父仓库 `difftest.py` **768 例全过**
（原 704 + 新增 64 条回归用例）、`fuzz.py` 120 例 0 失配；本仓库 `cargo test` **135 条全绿**，
`live_hits` 12 条无退化。

---

## 8. 验证

### 8.1 逐点对照真二进制（24/24）

用 `analysis/gwemu.py::Gw.call_path` 直接跑 `0x3c2530`，与两份 Python 复刻逐点比较
（用例都按真链路先加 `correction`，否则第一点就因弦长不足而只出 1 点，属于平凡一致）：

| case | step | flip | 真点数 | `base`（分歧索引，**0 基**） | `hi2` |
|---|---|---|---|---|---|
| sin x / cos x / abs x / x² / x³ / 1÷x / tan / exp / √x / log x / ln x / sin·x | 0.5 | F | 27–103 | ✅ | ✅ |
| steep20 `sin(20x)` | 0.5 | F | **401** | ❌ `DIFF@3` | ✅ |
| steep50 `sin(50x)` | 0.5 | F | **174** | ❌ `DIFF@2` | ✅ |
| sym centre `abs(sin(30x))*3` | 0.5 | F | **401** | ❌ `DIFF@4` | ✅ |
| flip sin / flip 1÷x / flip tan / flip x² | 0.5 | **T** | 28–73 | ✅ | ✅ |
| flip steep20 | 0.5 | **T** | **401** | ❌ `DIFF@1` | ✅ |
| step 0.1 sin(3x) | 0.1 | F | 511 | ✅ | ✅ |
| step 0.1 sin(20x) | 0.1 | F | **2001** | ❌ `DIFF@19` | ✅ |
| step 0.1 abs(sin(7x))*2 | 0.1 | F | **2001** | ❌ `DIFF@25` | ✅ |
| doc89 heavy | 0.5 | F | **401** | ❌ `DIFF@4` | ✅ |

**合计：`base` 17/24，`hi2` 24/24。** `hi2` 的「一致」是**整条点序列逐位相等**，不是统计吻合。

### 8.2 `docs/89` §11 的四个 oracle 数字

`hi2` 给出 `(401, 24.809820, 9.871885)` 与 `(20001, 23.212776, 6.417558)`，
与 `docs/89` 记录的 `(24.8098, 9.8719)`、`(23.2128, 6.4176)` 逐位一致。

### 8.3 反汇编器的交叉核对

| 地址（Ghidra） | objdump | Ghidra 12.1.4 独立解码 |
|---|---|---|
| `0x4c2a0a` | `movsd %xmm0,0x50(%rsp)` | `MOVSD qword ptr [RSP + 0x50], XMM0`（字节 `f20f11442450`） |
| `0x4c2a43` | `ucomisd %xmm0,%xmm1` | `UCOMISD XMM1, XMM0`（字节 `660f2ec8`） |
| `0x4c2a4d` | `jbe 0x3c2955` | `JBE 0x004c2955`（字节 `0f8602ffffff`） |
| `0x4c2955` | `dec %ebx` | `DEC EBX`（字节 `ffcb`），incoming refs = `0x4c2a4d`(条件) 与 `0x4c2a95`(无条件) |

两个独立解码器一致 ⇒ 不存在「objdump 从半条指令起解」的风险。

### 8.4 复现

对照实验已收进本仓库：**`tools/bisect_rules.py`**（两个变体 `base` / `hi2` 都实现了）。

```bash
cd gw2-rebuild
python3 tools/bisect_rules.py --trace                 # §6 的轨迹，不需要 unicorn
../.venv/bin/python tools/bisect_rules.py --oracle    # §8.1 的 24 例，约 30 s
../.venv/bin/python tools/bisect_rules.py --oracle --quick   # 只跑前 8 例

# 原始反汇编（注意必须从函数入口起解）
objdump -d --no-show-raw-insn --start-address=0x3c2530 --stop-address=0x3c2b5d \
  ../libgraphwarrust.so
```

`--oracle` 依赖分析仓库的 `.venv`（unicorn）与 `analysis/gwemu.py`，路径从 `tools/` 自动推出；
`--trace` 只用标准库 + 父仓库 `graphwar_curve.py` 的解析/求值。

---

## 9. 残留未知 / 边界（写清楚缺什么）

### 9.1 `chord2` 为 NaN 的那一瞬 —— 两个 Python 复刻与汇编不等价

`ucomisd` 的 `jbe` 在 unordered 时成立 ⇒ 汇编走「退回」；
Python 的 `ln <= ln2` 在 `ln2 = NaN` 时为 `False` ⇒ 走「接受」。
触发条件：`f(mid)` 有限但 `f(mid2)` 为 NaN（例如 `sqrt`/`log` 定义域边缘的窄带）。

- 汇编行为：**逐位验证（读指令）**；
- 这条差异在真游戏里是否真的发生、影响多大：**未实测**（现有 24 例没触发）。
- 建议按 §7.1 一起改掉（成本为零，且能让实现与指令完全同构）。

### 9.2 `0x90(%rsp)`（`lo` 备份）在复检段被 `chord` 覆盖

`0x3c2a13` 把 `chord` 写进 `0x90(%rsp)`，而「不满足」分支的 `hi = mid` 路径会在
`0x3c2a8c` 从 `0x90(%rsp)` 读回 `lo`。看似会读到垃圾，**实际不会**：

- `0x3c2a13` 之后只有两个出口：`0x3c2aa0`（接受，不再用 `0x90`）或 `0x3c2a4d → 0x3c2955`
  （回到 `0x3c295d`，第一件事就是 `movsd %xmm0,0x90(%rsp)` 重新写入 `lo`）；
- 而 `je 0x3c2b5d`（计数用尽）也不需要 `0x90`。

⇒ 这是编译器的槽位复用，不是语义。**逐位验证（读指令）+ 人工走全部出口**。

### 9.3 Ghidra 的函数边界

本次 Ghidra 把 `0x4c2a0a`/`0x4c2a43`/`0x4c2955` 都归到函数 `0x4c2530` 内，
没有出现 `docs/02` §12 那类边界错误。但**本文所有结论以 objdump 为准**，
Ghidra 只用于 §8.3 的字节级交叉核对；**没有**采信 Ghidra 的反编译文本。

### 9.4 本文写作时未做、**后来已补做**的事

- 父仓库 `analysis/difftest.py` / `fuzz.py` 回归：本文按任务约定只做静态分析、不改源码，
  回归由改码一方执行（`docs/AGENTS.md` 硬规矩 2）。**已补跑**：768 例全过、fuzz 120 例 0 失配
  （`docs/89` §32）。
- **没有**覆盖 `ode=True` 路径（本文只判定了「ODE 分支无复检」这一条）。
- 用真游戏实况复核：`docs/89` §26 显示实况 bug 的根源就在这里。**已重跑**
  `tools/sim_turns_analyze.py`：`docs/86` §14 给出 82 发并集 **152/152**、多报 0 漏报 0，
  §9.1 的「19/82 曲线被截断」消失。
- `tests/live_hits.rs` 切到 0.01：**已切、12 条全过**（`docs/89` §33）。当年切过去后 4 条失败
  **与本修复无关**，是测试自己跳过射手身体的半径固定 0.5、撞上掩膜像素量化抖动。
- 仍**未做**：`chord2 = NaN` 分支与汇编的等价性实测（§9.1）。

---

## 10. 对 `docs/89` 的更正

`docs/89` 的结论**方向对了、位置差一行**：

| `docs/89` 的说法 | 更正 |
|---|---|
| §29「`lo = x` 与二进制不符，两个取值都不对」 | `lo = x` **是对的**；漏掉的是同一分支的 `hi = mid2` |
| §17/§19/§20「差一次二分迭代 / 接受上一轮」 | 不是次数问题；是退回后**区间上界**不同。实测两边迭代数都 = 20 |
| §29「二进制接受 chord 略大于 0.5 的点，既不是退回起点也不是推进下界能产生的」 | 正是「退回起点 **且** 上界砍半」能产生的 |
| §5「`lo = mid` 会让曲线 4 点中止」 | 仍然成立（`lo = mid` 与 `0x3c2a4d` 的目标不符） |

按 `docs/INDEX.md` 的规矩「一个主题一份现行文档」，这些更正**本该直接改进 `docs/89`**。
本轮任务约定「不修改 `docs/89` 已有内容」，所以 `docs/89` 正文保持原样，
只在它的末尾追加了 §30 指向本文。**读 `docs/89` §29 时以本文为准。**
