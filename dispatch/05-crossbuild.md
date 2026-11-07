# 05 · `CG/crossbuild`：Windows / Android 编译验证

**优先级 P2。完全独立，不需要二进制。**

## 背景

Win/Android 的 `.gdextension` 条目**已经补过**，但这两个目标**从未编译过**。
跨平台能不能成立，现在完全是空的。

## 要做什么

1. 装 target：

```bash
rustup target add x86_64-pc-windows-gnu aarch64-linux-android
```

（Windows 需要 `mingw-w64` 链接器；Android 需要 NDK 的 `aarch64-linux-android-clang`。
**装不上就如实报告"在这台机器上做不了 + 缺什么"**，不要硬凑。）

2. 先 `cargo check --target ...`，再尝试 `cargo build --release --target ...`；
3. 逐条修平台相关问题，**修一条记一条**（`#[cfg(target_os)]`、路径分隔符、`libc` 差异、
   `std::os::unix` 的使用点、`.gdextension` 里的库名/后缀）；
4. 产出结论表：

```text
target                      check    build    失败原因（若有）
x86_64-pc-windows-gnu       ✅/❌    ✅/❌    ...
aarch64-linux-android       ✅/❌    ✅/❌    ...
```

5. 顺带核对 `.gdextension` 文件里的**库名与路径**是否与实际产物一致
   （Win 是 `.dll`、Android 是 `.so`、macOS 是 `.dylib`）。

## 允许改的文件（**只许这些**）

- `Cargo.toml`（target 相关配置）
- `src/**` 里**必要的** `#[cfg]`（不许改逻辑；每处都要能在 PR 说明里解释）
- `client/*.gdextension`
- **`docs/18-build.md`** 追加一节（含完整命令与失败原因）

⚠️ **不许改**：测试的既有断言、`docs/INDEX.md`、`docs/HANDOVER.md`。

## 验收

```text
cargo check --target <两个>      各自给出明确结果（过 = 过，不过 = 逐条原因）
cargo test --offline             仍 1499 全绿（本机默认 target 不受影响）
```

## 汇报（≤8 行）

分支 + sha、两个 target 的 check/build 结果、修了几处平台问题（各是什么）、
哪些是"这台机器缺工具链、做不了"、`cargo` 条数。
