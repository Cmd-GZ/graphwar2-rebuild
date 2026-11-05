> **已归档（2026-11 文档归并）**
>
> 本文内容已并入 [`06-verification.md`](../06-verification.md) §12。
> 本文件只作**过程档案**保留：原始证据地址、命令输出都还在，但**不要引用其结论**。
> 逐份判定与归并映射见 [`107-docs-consolidation.md`](../107-docs-consolidation.md)。

# 11 · 间接派发：补上调用图的盲区

## 1. 问题

做调用关系分析时踩到一个反复出现的现象：

- `0x3c2bc0`（开火结算）**本地索引里零调用者**，Ghidra 的 `procedure_callers` 也是空；
- `0x5eb57d`（掩膜测试的薄包装）同样零调用者。

原因不是它们没被用，而是**经函数指针调用的**：Rust 的 `dyn Trait`/闭包会被编译成
".data.rel.ro 里一个指针槽 + 运行时间接跳转"。静态调用图与 Ghidra 的 caller 列表都断在这里。

## 2. 方法：从重定位表反查指针槽

`analysis/relocs.pkl` 记录了 `R_X86_64_RELATIVE` 重定位（`槽位 -> 目标地址`）。
把目标地址反过来查，就能找到"哪个数据槽装着这个函数的地址"；再用 capstone 的
交叉引用找"谁引用这个槽"。

```python
slots = [slot for slot, adds in relocs.items()
         if any(typ == 8 and addend == TARGET for typ, addend in adds)]
users = {owner_of(insn) for slot in slots for insn, *_ in xrefs.get(slot, ())}
```

## 3. 结果（观测）

| 目标函数 | 指针槽 | 引用槽的函数 | 模块 |
|---|---|---|---|
| `calculate_path` `0x3c2530` | 1 | `0x3c2bc0` | `graphwar_shot_calculator.rs` |
| | | `0x3e2830` | **`graphwar_target_manager.rs`** |
| `shot_calculator` `0x3c2bc0` | 1 | `0x470eb0` | `graphwar_game_state_manager.rs` |
| | | `0x51a750` | **`graphwar_function_evolutionator.rs`** |
| `mask_test` `0x4f15b0` | 0 | — | 无指针槽，调用者全是直接调用 |

## 4. 两个新结论（观测 + 推断分开）

**观测**：`calculate_path` 被 `shot_calculator` 与 `target_manager` 两处间接调用；
`shot_calculator` 被 `game_state_manager` 与 `function_evolutionator` 两处间接调用。

**推断（未证实）**：

1. **`function_evolutionator` 会"模拟开火"**。它拿到的是整个开火结算函数，
   而不是某个便宜的小工具——说明 AI 在**批量试射候选函数**来评分，
   这与模块名"进化"一致（对表达式做变异/选择）。若成立，AI 的搜索空间
   与玩家输入走的是同一条代码路径，保真度要求也随之提高。
2. **`target_manager` 也会算曲线**（直接用 `calculate_path`）。
   它与"目标选择会写生命状态"（`docs/10` §2）拼起来，暗示它做的是
   **瞄准/威胁评估**：算出候选曲线，看会打到谁。

两条都**没有**验证到参数层面——需要看这两处在调用前后怎么用返回值。

## 5. 方法学结论

- "谁调用谁"这件事，**只看直接调用会得出错误结论**（会以为某函数是死代码）。
- 补法：重定位表反查指针槽 → 交叉引用找使用者。
- 仍然有盲区：经**寄存器/结构体字段**传递的函数指针（不是静态槽位）依然看不到。
  所以任何"没有被调用"的判断都必须写成"**未观测到调用**"，而不是"没有调用"。

这条规则已同步到 `docs/00-status.md` 的方法学陷阱里。
