# 17 · 游戏资源还原（rsc/）

## 1. 做了什么

从 `Graphwar.pck` 直接解包到 `rsc/`，**按原始路径**组织，并生成 `rsc/MANIFEST.tsv`
（路径 / 大小 / SHA-256）。

```bash
../.venv/bin/python tools/pck_extract.py --verify
```

实测输出：

```
PCK: Graphwar.pck  godot 4.7.0  pack v4  文件 692 个  file_base=0x70
写出 692 个文件, 共 19.6 MiB, 清单 rsc/MANIFEST.tsv
```

`--verify` 会对每个文件校验 PCK 里记录的 **md5**，692/692 全部相符。

## 2. 为什么不做格式转换

`.ctex`（压缩纹理）、`.scn`（二进制场景）、`.gdc`（GDScript 字节码）、`.res` 等
**本身就是 Godot 可直接加载的资源格式**。转成 PNG/文本/JSON 会丢信息，
而且转换器自己也需要逆向——等于把问题变大。
所以这里只做"忠实解包"，还原的是**可被 Godot 使用的资源**。

## 3. PCK v4 的格式（实测，不是照抄文档）

| 偏移 | 内容 |
|---|---|
| `0x00` | magic `GDPC`（`0x43504447`） |
| `0x04` | pack_version = **4** |
| `0x08/0x0c/0x10` | Godot 版本 4.7.0 |
| `0x14` | pack_flags（本包 = 2） |
| `0x18` | `file_base` = `0x70`（文件内容起点） |
| `0x20` | `directory_offset` = `0x138f440`（**文件索引在包尾**） |
| `0x28..0x68` | 保留 |
| `dir_offset` | `u32 file_count`，随后每项：`u32 path_len`、path（4 字节对齐）、
`u64 offset`、`u64 size`、`u8 md5[16]`、`u32 flags` |

**一个踩过的坑**：我一开始按 v2 解析（索引紧跟头部），得到 `file_count = 0`。
v4 把索引挪到了包尾，头部多了 `directory_offset`。

**另一个坑**：偏移的基准是 `file_base`，**不是包起点**。本包 `pack_flags = 2`，
按 Godot 源码里 `PACK_REL_FILEBASE` 的字面意思应该相对包起点，但我实测
`base = 0` 时 692 个文件 md5 **全部不符**，`base = file_base` 时**全部相符**。
所以代码里以实测为准，并把这段写进注释——**标志位的语义不能想当然**。

## 4. 资源构成（观测）

| 类型 | 数量 | 说明 |
|---|---|---|
| `.import` | 184 | Godot 导入配置 |
| `.ctex` | 167 | 压缩纹理 |
| `.remap` | 158 | 资源重映射 |
| `.gdc` | 105 | GDScript 字节码（UI 脚本） |
| `.scn` | 31（其中 **6 个是 AdMob 插件场景** ⇒ 游戏场景 **25** 个，`92` §2.3） | 二进制场景 |
| `.res` | 22 | 二进制资源 |
| `.fontdata` / `.mp3str` / `.sample` / `.cfg` | 7 / 6 / 3 / 3 | 字体、音频、配置 |

与 `01` 的结论一致：**玩法逻辑不在这些脚本里**，只有 UI。
后来 `92`/`101` 把这条坐实了：51 个非插件脚本里只有 5 个有行为，且没有一条是游戏规则。

## 5. 为什么不入库（重要）

`rsc/` 已加入 `.gitignore`。两个理由：

1. 上游 `AGENTS.md` 的硬性规则：**不提交二进制衍生资源**（`*.pck`、`pck/` 等），
   它们体积大、可再生，而且会把游戏发行商的资源带进版本历史；
2. `Cmd-GZ/graphwar2-rebuild` 是公开仓库。把这些资源推上去等于**再分发游戏素材**，
   这是发行商的版权范围，不是我们能替他决定的。

需要时可一键再生（`tools/pck_extract.py`），所以不入库不损失可复现性。

**如果你确认有权提交**（例如仓库改为私有、或仅内部使用），告诉我，我可以：
- 把 `rsc/` 从 `.gitignore` 移除并提交；
- 或改为提交**清单**（`MANIFEST.tsv` 只有路径/大小/哈希，不含内容）。

> 当前状态：**已提交 `MANIFEST.tsv`**（清单不含资源内容），`rsc/` 本体保持本地。

## 6. 下一步：让重建结果真的能构建成二进制

资源与控制流是两条独立的线。二进制那边要做的是：

1. 把 `gw2-rebuild` 从普通 crate 变成 **cdylib**，导出 Godot 要求的
   `gdext_rust_init`（原 `.so` 的**唯一**动态导出就是它，已用 `nm -D` 确认）；
2. 用真实的 godot-rust（或手写 GDExtension ABI）让它能在 Godot 里被加载；
3. 构建产物用 `readelf` / `nm` 验证：是 ELF 共享对象、导出了入口符号。

这条线还没开始，本章只记录到"资源还原完成"。
