# 53 · 找到了：关卡生成用 ChaCha，状态可运行时读出

## 1. 关卡生成算法（Ghidra 反编译 `FUN_004f1e50`）

`FUN_00566c40` 里带源文件字符串 `src/graphwar_level_generator.rs`，它调用
`FUN_004f1e50` —— **这就是关卡生成函数**：

```c
void FUN_004f1e50(undefined8 ctx, int min_count) {
  FUN_004f19a0();                                   // memset(mask, 0xff, len)：清空掩膜
  n = (int)(rand_f64() * 7.0 + 15.0);               // ← 障碍个数
  if (n <= min_count) n = min_count;
  while (n-- > 0) {
    r = rand_f64() * ? + ?;                         // 半径（见 §4）
    if (r <= 0.8) r = 0.8;                          // 下限钳位
    u = (rand_u64() >> 11) * 2^-53 - 0.5;           // [-0.5, 0.5)
    x = (2*r + 25.0) * (u + u);
    v = (rand_u64() >> 11) * 2^-53;
    y = (2*r + 15.0) * (v - 0.5 + v - 0.5);
    place_obstacle(r, ctx, (x, y));                 // FUN_004f17a0
  }
}
```

- 数量公式常量已从 `.rodata` 读出：`0x19f7e8 = 7.0`、`0x19fd40 = 15.0` ✓；
- 实测障碍数 15~21，与 `[15, 22)` 吻合；
- `x ≤ 2r+25`、`y ≤ 2r+15`，与观测到的坐标越界范围吻合。

## 2. RNG 是 ChaCha（块函数 `FUN_00850f30`）

反编译后可见：

- 从 `.rodata` 取 `s_expand_32_byte_k_0029cb40`（σ 常量）；
- 标准的 `<<12/>>20` 与 `<<7/>>25` 旋转（ChaCha quarter-round）；
- 状态布局：`param_1[0..7]` = **32 字节密钥**，`param_1[8..9]` = **64 位计数器**，
  `param_1[10..11]` = nonce；一次生成 4 个块（256 字节）。

⇒ 就是 `rand_chacha`。之前 `docs/40`/`docs/48` 的"OS 熵不可复现"结论到此**彻底作废**：
不是没有 RNG，而是我们之前找错了地方。

## 3. 运行时能读到 ChaCha 状态（criterion 1 达成）

hook 取随机 u64 的函数 **`0x47f700`**，其第一个参数是 RNG 状态指针，
读出 12 个字：

```
key : d708b994 5f635d20 092bc372 241bf92a a948e56f 1a22a2e7 2bdff82f 09e7bcd8
ctr : 1053b1cc 0011c2e7 998c066f 2a43257b
```

- **类型**：ChaCha（§2 的块函数确认）；
- **内存位置**：运行时地址（上述指针，每次运行不同）；
- **种子来源**：仍未知（未观测到 32 字节的 getrandom），但**已不影响重放**——
  只要能在运行时读出密钥与计数器即可。

工具：`tools/dump_rand_state.js`、`tools/capture_rng_and_obstacles.js`
（后者在同一次运行里同时抓 ChaCha 状态与真实 `AddObstacle` 报文）。

## 4. 尚未闭合：重放没对上（诚实记录）

用捕获的 key/counter 实现 ChaCha，按 `(x>>11)*2^-53` 取值去匹配
从真实障碍反推出的 `u`（`u = x/(2r+25)/2 + 0.5`）：**8/12/20 轮 × 4 个偏移全部未命中**。

最可能的原因（按可能性排序）：

1. **`BlockRng` 的缓冲区索引没算进去**：`rand_chacha` 的 `ChaCha12Rng` 是
   `BlockRng { core, results[64], index }`，我读到的 state 是"下一个块"，
   而当前消费位置在 `results[buffer..]` 里，状态指针 + 偏移才能看到；
2. 捕获的第一个状态可能不是关卡生成那一次（程序启动阶段还有其它随机消费）；
3. 半径公式的常数还没核实（`*1.6+2.6` 只能到 4.2，而实测有 5.13，说明伪代码里
   那两个常数我读错了位置），因此反推的 `u`/`v` 也可能整体偏移。

## 5. 下一步

1. 用 `FUN_004f1e50`（vaddr `0x3f1e50`）作为**触发点**：进入该函数时再读 RNG 状态，
   而不是取"第一次随机调用"；
2. 顺带 dump 状态指针后的 512 字节，找到 `results[64]` 缓冲与 `index`，
   直接读出"接下来会返回的字"，绕开 ChaCha 实现；
3. 用真实 `AddObstacle` 校准半径公式的常数（用 `r = t*a+b` 两点定参）。
