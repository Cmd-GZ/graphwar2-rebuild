# 61 · 关卡生成的 Rust 实现（逐位复现真实关卡）

## 1. 实现

`src/levelgen.rs`：

```rust
pub fn normal(next: &mut dyn FnMut() -> u64) -> f64   // ziggurat 标准正态
pub fn generate(next: &mut dyn FnMut() -> u64, min_count: i32) -> Vec<Obstacle>
```

公式与常量全部来自 `docs/55`/`docs/56`；ziggurat 的两张表由
`src/levelgen_tables.rs` 承载，**从 `.rodata` 原样导出**
（`ZIG_NORM_X` @ `0x25b428`、`ZIG_NORM_F` @ `0x25ac20` 起）。

## 2. 验证：逐位复现

夹具 `fixtures/levelgen_draws.txt`（真实抓包的随机流前 120 个值）
+ `fixtures/levelgen_obstacles.txt`（同一局真实发布的 13 个障碍）。

测试 `replays_captured_level_exactly`：从第 3 个随机数开始跑 `generate`，
对每个障碍比较 `x/y/r` 的 **`to_bits()`**（不是近似比较）：

```rust
assert_eq!(a.x.to_bits(), b.x.to_bits());
assert_eq!(a.y.to_bits(), b.y.to_bits());
assert_eq!(a.r.to_bits(), b.r.to_bits());
```

结果：**13/13 逐位一致**。

## 3. 一个未被观测的边界

ziggurat 的最后一个桶（`i == 255`）需要 `ZIG_NORM_F[i+2]`，而表只有 257 项。
真实游戏走这一桶时取什么值**我们没有观测到**（夹具也没触发），
所以实现里遇到就越界重抽，**不凭空造值**。这在代码里有注释说明。

## 4. 为什么这一步重要

服务端要能自己造关卡，就必须有可用的生成器。现在它既存在、
又被真实数据逐位验证过——不是"照着公式抄一遍"而已。
