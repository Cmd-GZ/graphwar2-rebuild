# fixtures/bit_exact_sweep_diffs.md —— T13 对拍发现的**真差异**（只报告，未改 `src/`）

> 生成：`../.venv/bin/python tools/bitexact_report.py --work .bitexact_work --write`
> 复跑：见 `docs/06-verification.md` §64 与 `fixtures/bit_exact_plan.txt` §D。
> 红线：本任务**不改** `src/**`（任务书 §4）。下面每条都能被 lead 直接派活修。

## D1  0x4f5450 `PolishNotationConverter::reorder_recursive` —— **无差异**

18 组输入（`fixtures/bit_exact_inputs.txt` 的 `[pnc]` 段）**全部逐位相同**，
包括 tag 18/19 括号、一元/二元平级、缺操作数补 `CONST 0`、深嵌套 8 层、
64 token 长序列（含 NaN/±inf/denormal/−0 常量）。

⇒ 该函数证据档位从 `观测` 升为 **`逐位`**（这是本轮唯一升档的函数，见 TSV 的 `跑成` 行）。

## D2  求值器 0x4eb9d0 / 采样器 0x3c2530 —— **无差异**（正对照）

求值器 20 组（含 ±0/±inf/qNaN/denormal/1ulp 邻居）全逐位相同；采样器 4 组点序列全逐位相同。

## D3  `MathFunction` 解析结果的 **非 CONST token 的 `value` 字段**：原版是**未初始化内存**，我方恒 `0.0`

**现象**（`[parse]` 段 14 组，tag 流 14/14 相同，`value` 字段 13/14 不同）：

| case | 表达式 | 原版 value（每 token） | 我方 value |
|---|---|---|---|
| p01 | `x` | `0x0000000000000004` | `0x0000000000000000` |
| p05 | `(x+y)*3` | 全部 `0x…08`（CONST 是 3.0） | 全部 `0x…00` |
| p06 | `sqrt(x^2+y^2)` | `0x…10` 与 `2.0` 混杂 | 全部 `0x…00` |
| p12 | `abs(x)+ln(x)+log(x)+tan(x)` | 全部 `0x…20` | 全部 `0x…00` |
| p14 | `pi*x` | 全部 `3.141592653589793`（= CONV 的 pi？见下） | 全部 `0x…00` |

**判据它不是我方写错**（三条，都可复跑）：

1. `value` 的取值随**输入串长度**单调变大（1 字符→`0x4`、7 字符→`0x8`、
   9–12 字符→`0x10`、30 字符→`0x20`），而且**同一个串里各 token 的值不同**
   ⇒ 不是「某个语义值被算出来」，是每次分配残留的不同字节数/长度类残渣。
2. 每例**换一台干净的 unicorn**（`Gw()` 新实例、堆页全新）结果**一模一样**
   ⇒ 不是本 harness 的堆复用假象（这条我先怀疑过，实测排除）。
3. `value` 只在 tag 17(`CONST`) 时被求值器/转换器使用；`reorder_recursive` 的
   18 组对拍里我喂**同一组** value 位给两侧，输出 100% 相同 ⇒ 逻辑没受影响。

**结论（分档）**：

* `逐位`：**tag 流**（14/14 相同，含 `pi`→`n`→CONST(π) 的替换链、按 777 的优先级重排）。
* `推断`：原版非 CONST token 的 `value` 是**未初始化内存残渣**。
  **缺的证据**：无法在真机进程里读到那块内存的绝对地址与内容；
  要坐实需在 arm64 未 strip 版上对 `FunctionToken::new` 的返回值逐字段下断点。
* **影响面**：语义无影响（求值器不看非 CONST 的 value）；但**字节级复现**
  （例如把 token 数组当报文/哈希输入）**不可能逐字节相同** —— 不要把这一项写进「逐位」。

**最小复现**：

```bash
export CARGO_TARGET_DIR=$PWD/.cargo-target
cargo build --release --offline -q --example bitexact_harness
../.venv/bin/python tools/bitexact_run.py real --mode parse \
    --in fixtures/bit_exact_inputs.txt --out /tmp/r.txt
../.venv/bin/python tools/bitexact_run.py ours --mode parse \
    --in fixtures/bit_exact_inputs.txt --out /tmp/o.txt
../.venv/bin/python tools/bitexact_run.py diff --real /tmp/r.txt --ours /tmp/o.txt
# 只有第 3 列之后的 value 位不同；把 : 后两段去掉再 diff 就是 0 条。
```

## D4  harness 自身的一处**假差异**（已修，记下来免得别人再踩）

第一版 `[eval]` 有 2 组不一致（`sin(±inf)+3` → 原版 `+qNaN` / 我方 `-qNaN`）。
真因是 `analysis/emu.py` 的 libm hook 用的是 **Python 的 `math`**（`_safe(math.sin)`，
异常被吞成 `float('nan')` = **+NaN**），而 glibc 的 `sin(±inf)` 给的是 **−NaN**。
改成 `ctypes` 直调 `libm.so.6` 后这 2 组**逐位相同**。
⇒ 教训：**正对照不过先怀疑自己的 hook，别急着报差异**。

---

## 附：本轮**没跑**的 438 个函数，按原因分类

见 `fixtures/bit_exact_sweep.tsv` 的 `跑没跑` / `档位` / `备注` 三列（439 行逐个点名）。

其中最值得下一轮先跑的一个（ABI 已初步读出、代价小、价值高）：

* **`0x4be3e0` `graphwar_plane.rs`**（87 条指令，`executable`：`in:rsi`）——
  `mov (%rsi),%rax` 取 tag，`xor 0x8000000000000000` 归一化后与 `0x28`/`0x29` 比；
  然后 `movsd 0x18(%rsi),%xmm0` → `call *0x9bb288` → `mulsd 0x19fbd0` → `divsd 0x19faa8`
  → `maxsd 0x19fbc0` / `minsd 0x19f7e0` → `cvttsd2si`；再对 `+0x8`/`+0x10` 两个 f64
  走**向量化** `mulpd 0x19d420` / `divpd 0x19ce00` / `addpd 0x19ee90` + 同样的 clamp。
  ⇒ 这是**纯数值**（`px = …`）变换，无引擎依赖，且 `0x9bb288` 是 libc 数学导入可 hook。
  缺的只是「rdi 是 `&mut self` 还是输出缓冲」那一步的落地判定。
