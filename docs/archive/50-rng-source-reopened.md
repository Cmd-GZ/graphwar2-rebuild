# 50 · 推翻"OS 熵"结论：随机源其实还没找到

`docs/40` 曾断言：关卡生成用 OS 熵（`getrandom` 921 次/40s、`rand/srand` 全 0）
⇒ 不可复现。**这个结论是错的**，本轮用调用栈把它推翻了。

## 1. 错在哪：那 921 次调用几乎都不是游戏发的

hook `getrandom` 并对**每次调用**取调用栈，按栈聚类（35 秒一局）：

| 次数 | 调用栈（前几帧） | 归属 |
|---|---|---|
| 865 | `libudev.so.1 < Godot < Godot …` | **libudev 枚举硬件** |
| 18 + 15 + 9 + 5 + 2 + 1 | 同样含 `libudev` | 同上 |
| **1** | `GW+0x907c66 < …` | 游戏本体 |
| **1** | `GW+0x907c66 < GW+0x55c86d < libc < GW+0x907be2 < GW+0x55de07` | 游戏本体 |
| **1** | `GW+0x907c66 < GW+0x907be2 < GW+0x6996a1 < GW+0x68cfc2 < GW+0x57a175 < …` | 游戏本体 |

⇒ **915 次来自 libudev**（Godot 的硬件枚举），**游戏本体只有 3 次**。

而且这 3 次全是 **16 字节**，调用点 `0x8bc0d3` 的代码是

```asm
8bc0d3:  call *0xfe6bf(%rip)   # getrandom
8bc0d9:  mov  %rax,(%r14)      # 存 8 字节
8bc0dc:  mov  %rdx,0x8(%r14)   # 再存 8 字节
8bc0e0:  movb $0x1,0x10(%r14)  # 置"已初始化"标志
```

这正是 Rust 标准库 **`HashMap` 的 `RandomState` 播种**（两个 u64 哈希密钥 + 标志位），
跟关卡生成没有关系。

## 2. 但 ChaCha 确实在二进制里

在 `.so` 里搜常量：字符串 **`expand 32-byte k` 出现 6 次**（ChaCha 的 σ 常量），
引用它的 SIMD 代码在 `0x7508f0`、`0x7524b4`、`0x64bb74`、`0x65ad8a` 一带。

⇒ 游戏**确实带了 ChaCha**（`rand_chacha`）。所以存在一个真正的 PRNG，
只是它的**种子不是通过 libc 的 `getrandom` 函数拿的**。

## 3. 为什么之前观测不到

Rust 的 `getrandom` crate 在 Linux 上走**原始 syscall**（`SYS_getrandom` = 318），
不经过 libc 的 `getrandom` 函数。我们 hook 的是**函数**，所以只看到了
libudev 这类用函数封装的调用方。

## 4. 试过但不可行：hook syscall 本身

想直接 hook libc 的 `syscall()` 来抓 318 号调用，**结果脚本挂死**：
frida 自身的通信也走 syscall，钩住它等于把自己的通道掐断。
这条路不可用（工具脚本 `tools/hook_syscall_rng.js` 保留，标注为不可用）。

## 5. 现状与下一步

| 问题 | 状态 |
|---|---|
| 关卡生成的随机数来自哪里 | **未知**（不是 libc getrandom，但二进制里有 ChaCha） |
| 是否可复现 | **重新变成开放问题**（此前判"不可"的证据不成立） |

可能的下一步：
1. 静态定位 ChaCha 的**初始化函数**（谁的参数里带 32 字节密钥），
   在运行时 hook 那个函数直接抓密钥；
2. 检查 `rand::rngs::StdRng::seed_from_u64`（SplitMix64），
   那种种子可能来自时间戳而非熵；
3. 用 Stalker 只对关卡生成函数做短程追踪，看它调用链里哪个函数产生浮点数。

## 6. 教训

`docs/40` 的推理是"看到大量 getrandom + 没有 rand/srand ⇒ 用 OS 熵"。
**这个推理跳过了"调用来自谁"这一步**，而 915/921 的调用其实来自无关的
硬件枚举库。以后凡是"某系统调用了 X"的结论，必须先看**调用栈**。
