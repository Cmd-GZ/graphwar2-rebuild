# 23 · 用真实对局验证复刻（采样器）

之前所有真值都来自静态夹具（`fixtures/*.tsv`）。现在有了活的游戏，可以从**真实对局**
里抓入参与输出，做同构比较。

## 1. 方法

`tools/capture_path.js`：frida 拉起 harness 并自动开局（Bot 开火），
在 `calculate_path`（RVA `0x3c2530`）上挂钩：

- **onEnter**：读入参 —— `rdi` 输出 Vec、`rsi` tokens Vec、`rdx` flip、`rcx` ode、
  `xmm0/1/2` = `x0`/`y0`/`step`；
- **onLeave**：读输出 Vec（`cap,ptr,len`）里的点序列，每个点 16 字节 `f64 x, f64 y`；
- Token 按 `{u64 tag, f64 value}` 共 16 字节解析。

产物：`.rea-work/captured_path.json`。

## 2. 结果（实测）

```
调用 0: x0=9.175978751627657 y0=-2.447450870359054 step=0.5 flip=True ode=False tokens=3
  真游戏 69 点 / 我们 69 点 → 逐点一致 69/69
调用 1: 同上 → 69/69
```

`python3` 侧用 `graphwar_curve.calculate_path(tokens, x0, y0, step, flip, ode)`
直接吃抓到的 tokens，逐点 `==` 比较。

## 3. 两个新观察

1. **step 不是常量 0.01**：这一局是 `0.5`。此前从二进制里确认的 `0x3f847ae147ae147b`
   是"服务端默认值"，实际调用点会传不同的步长。我们的实现接受 step 参数，
   所以两种都覆盖；但"游戏一律用 0.01"这个说法**不准确**，需要修正表述。
2. **士兵坐标是连续值**（`9.175978751627657`），不是整点 —— 与"换算成像素才取整"一致。

## 4. 意义

采样器这一行从"静态夹具验证"升级为"**真实对局验证**"，
而且覆盖了一条此前没测过的路径：非默认 step + flip=True。
