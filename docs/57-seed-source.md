# 57 · 种子来源：OS 熵（播种时机已定位，派生方式未还原）

## 1. 播种时机：第一次取随机数时惰性播种

hook `0x47f700`（取随机 u64），读其状态指针：

- **第一次调用时**：状态 8 个字**全为 0**（未初始化）；
- **同一调用返回后再读**：已是 32 字节随机 key。

⇒ 播种是**惰性**的，发生在第一次 `next_u64` 内部。

## 2. 事件顺序：getrandom 就在播种之前

同一次运行里同时 hook `getrandom` 与 `0x47f700`，事件序列：

```
0  getrandom len=16  GW+0x907c66
1  getrandom len=16  GW+0x907c66
2  getrandom len=16  GW+0x907c66
3  next_u64           ← 第一次
4  next_u64
...
```

- 三次 16 字节 = **48 字节**，足以提供 ChaCha 需要的 32 字节密钥；
- 调用点 `GW+0x907c66` 是**游戏自己的 getrandom 包装**（`docs/51` 已定位）。

⇒ **种子来源是操作系统熵**，经游戏自身的 `getrandom` 包装取得，随后用于初始化 ChaCha。

## 3. 未还原：字节 → 密钥的派生

把抓到的 16 字节种子拼起来，与读出的 key 比对：**不相等**。

```
key 字节: b0d7f318 b578eeeb 451ff966 19fa5814 7074d265 b7ce85a9 114568e9 4d272c1a
seed[3] : dab54fe8 9e6e7b3a 5ca21019 afc6c8bf
seed[4] : 19b3275d 35046228 a326ae14 a49a51cd
```

所以中间有一步**派生**（可能是 `SeedableRng::seed_from_u64` 的 PCG 展开、
或对种子做了哈希）。注意：该派生不影响 `docs/56` 的端到端重放——
重放只需要**运行时的 key**，不需要种子字节。

## 4. 判据 1 的三项对照

| 项 | 状态 |
|---|---|
| RNG 类型 | ✅ ChaCha（块函数 `FUN_00850f30` 用 σ 常量与标准 quarter-round） |
| 内存位置 | ✅ 运行时状态指针可读（key + 计数器） |
| 种子来源 | ✅ **OS 熵**（游戏自身 `getrandom` 包装 `GW+0x907c66`，播种前取得）；派生细节未还原 |

## 5. 复现

```bash
node tools/hook_seed_order.js     # 事件顺序：getrandom ×3 → 首次 next_u64
node tools/hook_seed_bytes.js     # 种子候选字节 与 首个非零 key
```
