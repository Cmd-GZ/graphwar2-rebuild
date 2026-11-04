# 20 · 活体 oracle：在真实游戏进程里调函数

## 1. 为什么这一步是分水岭

到此为止，所有真值都来自两种手段：

| 手段 | 能回答 | 不能回答 |
|---|---|---|
| 静态反编译（Ghidra） | 代码长什么样 | 它实际返回什么 |
| unicorn 模拟 | **纯函数**的返回值 | 需要真实堆/锁/Godot 状态的东西 |

现在有了第三种：**frida 注入到运行中的真实游戏进程**，可以直接调用
`libgraphwarrust.so` 里的任意函数（只要参数能用标量/指针表达）。

## 2. 环境准备（踩坑记录）

REA 自带的 frida 工具报 `Could not locate the bindings file`——因为当初 npm 的
`allowScripts` 策略拦了 frida 的安装脚本。修复：

```bash
cd .rea-runtime
npm install-scripts approve frida     # 放行
npm rebuild frida --foreground-scripts # 触发 prebuild-install 下载预编译绑定
# → node_modules/frida/build/frida_binding.node
```

注意：**REA 的 MCP 进程会缓存启动时的失败**，所以它那边的 frida 工具仍然不可用；
本仓库的做法是**绕过 REA、直接用 frida 的 Node API**（`tools/frida_live.js`）。

另一个坑：每次 bash 调用是独立 PID 命名空间，`enumerateProcesses()` 看不到
上一次调用启动的进程。所以必须用 **spawn 模式**：让 frida 自己拉起游戏，
再从同一进程树里 attach。

## 3. 工具

```bash
node tools/frida_live.js modules
node tools/frida_live.js call <rva-hex> <ret-type> [arg-type:value,...]
```

`rva` 用**虚地址**（`base + rva`）。注意 Ghidra 地址与虚地址差 `0x100000`：
Ghidra 的 `FUN_00a302b0` → 虚地址 `0x9302b0`。

实测（真游戏进程内）：

```
$ node tools/frida_live.js modules
{"gw":{"name":"libgraphwarrust.so","base":"0x7f44e1e00000","size":10241312}}

$ node tools/frida_live.js call 0x9302b0 double double:2.7   → 2
$ node tools/frida_live.js call 0x9302b0 double double:2.3   → 2
$ node tools/frida_live.js call 0x9302b0 double double:-2.7  → -3
$ node tools/frida_live.js call 0x9302b0 double double:8.718 → 8
```

## 4. 顺带修正一个结论（第二次自我纠错）

此前 `docs/08` 说 `0xa302b0` 是 `round`（错），后来按掩膜反推说"截断"。
**活体调用的结果是 `floor`**：`2.7→2`、`-2.7→-3`。

对光栅化没有影响——那里的输入是 `sqrt(...) ≥ 0`，`floor` 与向零截断等价，
所以 `docs/08` 的像素结论仍然成立；但函数身份必须写准。

## 5. 现在能做什么 / 还不能做什么

**能**：
- 用标量参数调用任意内部函数，读返回值；
- 枚举模块、读写内存、hook 函数看调用序列。

**还不能**：
- 直接构造 Rust 的 `Arc`/`Vec`/`StringName` 作为参数——需要先在进程里造出合法对象；
- 驱动游戏进入对局（headless 无输入），所以回合/AI 的**调用序列**还看不到，
  只能主动"喂参数"去问单个函数。

## 6. 下一步

用这套能力给回合模块造 oracle：先找那些参数是标量、返回是标量的函数
（例如回合计数、存活查询），逐个问出行为；对需要结构体参数的，
用 `Memory.alloc` + `NativeFunction` 手工构造最小对象。
