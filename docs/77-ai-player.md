# 77 · 结构还原版 AI（`src/aigen.rs`）

## 1. 它是什么

把前几轮还原出来的零件拼成一个能跑 `evolve` 的 AI：
**哪些是实测、哪些是假设，逐条标在下面**。

```rust
pub struct Params { generations, population, elite, limit }
pub fn params(level: u8) -> Params            // 六档实测参数
pub fn random_function(next) -> Vec<Token>    // 长度公式 + 抽 token + 修复
pub fn repair(tokens, next)                   // ⚠️ 假设
pub fn crossover(a, b, next)                  // ⚠️ 部分假设
pub fn mutate(tokens, next) -> bool           // 观测（docs/73）
pub fn next_generation(pop, params, next, score) -> Vec<Individual>
```

## 2. 实测部分（可直接当事实）

| 项 | 值 | 出处 |
|---|---|---|
| 六档 `(代数, 种群, 精英, 上限)` | `(3,50,5,5)…(1000,250,5,200)` | 实参直读 `docs/67` |
| token 权重表（总和 85） | 见 `docs/71` | 二进制 + 40 万样本 |
| 随机函数长度 | `trunc(N(0,1)×5+10)` | `docs/76` §2.5 |
| 抽多少发多少 | 35872/35872 完全相等 | `docs/76` §4.5 |
| 变异 | 常数乘 `(1+N)`/换新；值 token 包一层 | `docs/73` |
| 代际构成 | 工厂 0.8×种群，其余直接复制，精英 5 | `docs/75` |
| 父代太短的重生阈值 | 第二个父代 `< 20` token | 反汇编 `cmp $0x14` |
| 父代选择 | 均匀随机（`rand % N`），**无轮盘赌** | 反汇编 + 实测 |
| 适应度 | `命中×2e6 − 1e6 − V²/100 − (V>limit?5e7:0)`，**`V` = token 数、常规对局无友军项** | `docs/74` |

## 3. 假设部分（明确标注，未证实）

| 项 | 我的做法 | 缺什么 |
|---|---|---|
| **修复规则** | 按 GW1 `adjustFunction`：走到合法即截断，缺操作数就补常数 | GW2 的修复代码没定位到（`docs/76` §4.5 的两个负结果） |
| **交叉切点** | 各取一段拼起来 | 真正切点规则未还原（二进制在 `FUN_003be7d0`） |
| **20% 副本的选取** | 精英 5 个 + 其余从分数靠前者里随机取 | "除精英外的选取规则"未还原 |
| ~~适应度的 `X`~~ | **已查清**（`docs/74` §1.1）：`X` 是采样步长，那一项常规对局恒为 0，公式里没有友军惩罚 | — |

## 4. 测试（16 条，`cargo test aigen`）

- `weights_sum_to_85` / `distribution_matches_table`（17 万次抽样）
- `constants_are_normal_scaled_by_10_2`
- `random_length_is_normal_10_5`（均值≈9.5、σ≈5）
- `mutate_*`（5 条，含 300 轮合法性不变量）
- `repair_fixes_arbitrary_sequences`（200 组随机输入修复后都合法）
- `random_function_is_always_well_formed`（500 次生成）
- `params_match_measured_table`
- `next_generation_keeps_size_and_elites`

## 5. 还差什么才能"真的下棋"

1. **适应度函数本身**（`docs/74` 的公式要接上弹道采样 `path`/`settle` 与士兵坐标）；
   其中 `X` 的语义待子 agent 结论；
2. **修复/装配规则**（假设项第一条）；
3. 把 `next_generation` 按 `generations` 轮起来，取分数最高的个体开火。
