# 132 · 官方 Android / Windows 包：广告与内购的真实链路、跨平台对照、跨平台构建

状态：**现行**。本文只写三份**官方发行包**里能直接读到的东西，Python 侧不做任何"看起来对"的补全。
证据等级按 `AGENTS.md` 四档：**逐位验证 / 观测 / 推断（写明缺什么）/ 未知**。

> 一句话：**广告与内购是"外观 + 广告开关"，不进内核**；三平台的**游戏数据逐字节相同**（PCK 里的
> 452 个资产 md5 全等）；**唯一找到的平台条件常量是 AdMob 广告位 id**；官方 Windows 包 41 MB
> 是**编译配置 + 符号表**撑起来的，不是内容更多。

---

## 0. 一页结论

| 问题 | 结论 | 等级 |
|---|---|---|
| 广告/内购代码在哪 | **三平台都有**：Linux 的 `Graphwar.pck` 里同样带着 `Ad_holder.gdc` / `IAP_holder.gdc` / `android_iap.gdc`，`main.tscn` 里同样有 `GraphwarAdSwitch` 节点 | 逐位验证（md5 相同） |
| Linux 上为什么不工作 | 缺 AdMob 的 `MobileAds` / `AdView` 类与 `AndroidIAPP` / `IOSStore` 单例（.gd 里是 `Engine.has_singleton` + 直接 `MobileAds.initialize()`） | 观测（.gd 原文）+ 推断（未被任何脚本挡掉） |
| `ad_switch` 调了什么 | 只注册了 **2 个**方法：`are_ads_enabled()` 与 `get_ad_unit_id()`；前者 = `!owned`，后者返回**编译期常量字符串** | 观测（反汇编 + 方法注册表） |
| `backend` 是不是广告 | **不是**。`GraphwarBackend` 是**联机层**（LAN 广播 / 起服务端 / 起客户端 / 连回放 / broker），且**不是 Godot 类**（.rodata 里没有它的类名） | 观测（符号表 + 字符串） |
| `purchase_confirmed` "Android 2 处 / Linux 1 处" | **是 `.symtab` 造成的错觉**：Android 未 strip，多出来的那一处是**符号名** `...GraphwarRootNode18purchase_confirmed...`；`.rodata` 里两边都只有 1 处 | 逐位验证（字节计数） |
| `unlock_skins` / `purchase_graphwar_support` 解锁什么 | 商品 id 是 **`graphwar_support`**；确认后 ①`GraphwarPlayerConfig` 解除锁定（皮肤/脸/帽子可选）②玩家页 `SkinBlock` 隐藏 ③`GraphwarAdSwitch.owned = true`（广告关） | 观测（.gd + 反汇编 + .tscn 文案） |
| 解锁状态存哪 | **`user://player.config`**（`GraphwarPlayerConfig::load/save`） | 观测 |
| 影响玩法吗 | **不影响内核**。皮肤/脸/帽子只被 `graphics_manager` / `event` / `player_screen_control` / `texture_loader` / `player_config` / `game_state_manager` 引用；**采样器 / 掩膜 / 命中 / 生命 / 目标 / 回合里一次都没有** | 观测（符号归属 + 交叉引用） |
| 有没有影响玩法的平台分支 | **没找到**。三平台 PCK 资产逐字节相同；`.rodata` 字面量差异里没有一个是玩法参数 | 观测 + 推断（见 §3.3 的口径限制） |
| Windows 为什么 41 MB | `.text` 21.7 MB（**48 853** 个函数的展开数据）＋ **14.19 MB COFF 符号表**；Linux 是 6.1 MB `.text` / 13 430 个函数 / 已 strip | 观测（节表 + 头字段） |
| **Android 上广告怎么才会出现** | 唯一的开关是"有没有买 `graphwar_support`"（`are_ads_enabled() = !owned`，**没有**平台/地区/同意判断）；广告位是**发布者的真实位**、写死在 `.so` 里（**不是**测试位）；只在 **12 个界面**出现，**对局中不出现**；游戏**不弹 UMP 同意框** | 观测（§2.8） |
| 跨架构逐位一致 | **未验证**，本文不承诺 | 未知 |

---

## 1. 三份官方包的坐标

| 平台 | 文件 | 大小 | sha256（前 16） | 引擎 |
|---|---|---|---|---|
| Linux | `libgraphwarrust.so` | 10 227 896 | `9a53811518e74b75` | Godot 4.7.stable |
| Android | `.reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so` | 14 979 192 | `8a612e954a030f86` | Godot 4.7.stable |
| Windows | `Win/graphwarrust.dll` | 41 259 197 | `ab490af668612248` | Godot 4.7.stable |

游戏版本三份都是 **`2.0.60`**（Android `assets/project.binary` 里 `application/config/version`；
Linux/Windows 的 `project.binary` 与之**逐字节相同**，见 §3.1）。

### 1.1 三个"能干活"的事实（先记下来，会反复用）

1. **Android 的 `.so` 没有 strip**：`readelf -sW` 有 **107 568** 条符号，其中 **18 549** 条 `FUNC`，
   Rust 名字完整（`_ZN12graphwarrust18graphwar_root_node16GraphwarRootNode18purchase_confirmed17h...E`）。
   **本任务 90% 的结论都来自这张符号表**——它比 Ghidra 快得多，也不用抢共享会话。
2. **Windows 的 DLL 带整张 COFF 符号表**：`PointerToSymbolTable=0x19d1800`、`NumberOfSymbols=147444`，
   后面还跟 11.5 MB 字符串表。
3. **Linux 的 `.so` 已 strip**（无 `.symtab`），只能靠字面量交叉引用反推。

### 1.2 地址换算（三份包各不相同，别套用 `AGENTS.md` 的 +0x100000）

| 包 | `.rodata` 虚地址 vs 文件偏移 | `.text` 虚地址 − 文件偏移 |
|---|---|---|
| Linux `.so` | `0x19c000` == `0x19c000`（相同） | `0x1000` |
| Android `.so` | `0x179000` == `0x179000`（相同） | **`0x4000`** |
| Windows DLL | `.rdata` 文件偏移 `0x14c4400`（PE 节表） | `.text` 文件偏移 `0x400` |

> ⚠️ `AGENTS.md` 的"Ghidra 地址 = 虚地址 + `0x100000`"只对 **Linux 那份**成立。
> 拿 Android 的地址去 REA/Ghidra 里查之前必须先按上表换算，否则会命中无关函数
> （正是 `AGENTS.md` 记的那个"静默按文件偏移重解释"的坑）。

**方法说明**（可复现）：字面量交叉引用是用 capstone 线性扫描得到的——
aarch64 认 `ADRP+ADD/LDR` 对（`ADRP` 的 imm 即页基址），x86-64 认 RIP 相对寻址
（`insn.address + insn.size + disp`）。脚本只写在 `.agent-work/`，不入库；
每个结论都给了地址，用 `llvm-objdump -d --start-address=… --stop-address=…`（aarch64）
或 `objdump -d`（x86-64）可当场复核。

---

## 2. 广告 / 内购的真实链路

### 2.1 先破一个误解：Linux 包**也**带着这套东西

`Graphwar.pck`（Linux）里同样有：

```
Ad_holder.gdc  IAP_holder.gdc  android_iap.gdc  IAP_IOS.gdc  AdContainer.gdc
main.tscn.remap（main.tscn 里有 IAP_holder / IAP_Android / IAP_IOS / Ad_holder / GraphwarAdSwitch 五个节点）
Graphwar.gdextension（三平台同一份，见 §4.1）
```

而且 Android 的 `assets/` 与 Linux 的 `Graphwar.pck` 里 **452 个资产 md5 全等、0 个不同**
（`only-in-android` 只有 `_cl_`、`assets.sparsepck` 两个打包产物）。所以
"Linux 端跑不到"的原因**不是资源缺失**，而是它调用的 Android/iOS 插件在 Linux 上不存在：

| 脚本 | 依赖的东西 | Linux 上有吗 |
|---|---|---|
| `Ad_holder.gd` | `MobileAds.initialize()`、`AdView`、`AdSize`、`AdPosition`、`AdRequest`、`AdListener` | 无（AdMob 插件是 Android/iOS 的 GDExtension） |
| `android_iap.gd` | `Engine.get_singleton("AndroidIAPP")` | 无 → 只打一行 `AndroidIAPP singleton not found` |
| `IAP_IOS.gd` | `IOSStore` 单例 | 无 |

等级：脚本原文 = **观测**；"Linux 上因此不生效"= **推断**（缺：一次 Linux 实机日志，
应当出现 `AndroidIAPP singleton not found` 与 `MobileAds` 解析失败）。

### 2.2 调用链（全部有地址）

```
main.tscn
├── IAP_holder (Control, IAP_holder.gd)          ← ready() 里把 $IAP_Android / $IAP_IOS 的三个信号转发出来
│   ├── IAP_Android (Node, android_iap.gd)       ← Engine.get_singleton("AndroidIAPP")，商品 id = "graphwar_support"
│   └── IAP_IOS     (Node, IAP_IOS.gd)
├── Ad_holder (Control, Ad_holder.gd)
│   └── GraphwarAdSwitch (type="GraphwarAdSwitch")   ← Rust 类，Ad_holder.gd 直接 get_node 拿它
└── (RootNode / GraphwarRootNode)                ← 在 ready() 里把两个 holder 存下来并 connect 信号
```

`GraphwarRootNode::ready`（Android `0x39a6f0`，函数体 ≈ 5.6 KB）：

| 位置 | 做了什么 | 证据 |
|---|---|---|
| `0x39b5d8` | `GraphwarNodes::get_node_by_name_rec(…, "IAP_holder", 10)`（`0x19a9d5`） | 观测 |
| `0x39b648` | `…("Ad_holder", 9)`（`0x19a9df`），结果存进 self 的句柄槽 | 观测 |
| `0x39b850` | `Callable::from_object_method(self, "request_permissions_result", 26)` | 观测 |
| `0x39b8b4` | `connect_flags(<Node 的 tree>, "on_request_permissions_result", 29, callable, 1)` | 观测 |
| `0x39b8e8/914/940` | `Callable::from_object_method(self, "purchase_confirmed"/18, "purchase_pending"/16, "purchase_denied"/15)` | 观测 |
| `0x39b97c / 0x39b9ac / 0x39b9dc` | 句柄非空时，对 **IAP_holder** `connect_flags("purchase_confirmed"/"purchase_pending"/"purchase_denied", callable, 1)` | 观测 |

三个事件处理函数（符号表里有名字、有大小）：

| 函数 | 地址（Android） | 干了什么 | 证据 |
|---|---|---|---|
| `GraphwarRootNode::purchase_confirmed` | `0x3a64e4`（552 B） | ①`GraphwarPlayerScreenControl::unlock()`（`bl 0x450600`）②`GraphwarPlayerConfig::unlock()`（`bl 0x392964`）③`bind_mut(ad_switch)` → `[obj+0x18] = 1` ④如果 Ad_holder 句柄在 → `Ad_holder.call_deferred("destroy_ad_view", 15)`（`0x3a65d8`） | 观测 |
| `GraphwarRootNode::purchase_denied` | `0x3a610c`（480 B） | ①`…ScreenControl::lock()` ②`…PlayerConfig::lock()` ③`bind_mut(ad_switch)` → `[obj+0x18] = 0` | 观测 |
| `GraphwarRootNode::request_permissions_result` | `0x3aae7c`（800 B） | 权限回调（未逐指令展开） | 观测（存在）／未知（语义） |

`purchase_pending` **没有** Rust 方法——`IAP_holder.gd` 把 Android 侧的 `purchase_pending`
转发成自己的同名信号，RootNode 里对应的处理发生在 `GraphwarPlayerScreenControl::pending()`
（`0x450744`，324 B，与 `lock`/`unlock` 同尺寸）。等级：**推断**（缺：pending 分支的调用点交叉引用）。

### 2.3 `ad_switch` 到底调了什么

`graphwar_ad_switch` 在全二进制里只有 **6 条 `FUNC` 符号**，但真正的方法体藏在 gotdot-rust 的
注册闭包里。办法是把注册链走完：

```
ImplementsGodotApi::__register_methods        0x43fb5c  (400 B)  ← 只是遍历静态注册表
  └ FnOnce::call_once                         0x4576c4          ← 方法 1 的注册
       StringName::from("are_ads_enabled", 15)  ← 0x19dda0
       ClassMethodInfo::register_extension_class_method(ptrcall=0x4492c8, varcall=0x449364)
  └ （接着同一闭包里注册方法 2）
       StringName::from("get_ad_unit_id", 14)   ← 0x19dd92
       …(ptrcall=0x44922c, varcall=0x449424)
```

两个 `ptrcall_fn` 都把真正的方法体内联在 `godot_core::private::handle_fallible_call` 的实例里：

| 方法 | 实例地址 | 实现（逐指令） |
|---|---|---|
| `are_ads_enabled() -> bool` | `0x377cf4` | `borrow_mut` → `ldrb w22,[obj+0x18]` → `eor w8,w22,#1` → 写回 out 参数。**= `!self.owned`** |
| `get_ad_unit_id() -> GString` | `0x36f3d0` | `GString::from_str(<字面量>, 38)`；字面量地址 `0x19dd6c` |

**唯一确认的平台条件常量**（同一份源码、不同 target，长度都是 38，只差字节）：

| 平台 | `.rodata` 地址 | 内容 |
|---|---|---|
| Android | `0x19dd6c` | `ca-app-pub-2005941169362714/5433871375` |
| Linux | `0x1c43ff` | `ca-app-pub-3940256099942544/6300978111` |
| Windows | （`.rdata` 内 `ca-app-pub-` 出现 1 次，未逐字节取） | 未取 |

Linux 那串是 Google 官方的**测试横幅**广告位，Android 那串是发布者的真实广告位
⇒ 源码里这一定是个 `cfg(target_os)` / 构建脚本常量，**只在广告 SDK 层生效，与玩法无关**。
等级：Android/Linux 两串 = **观测**；"由 cfg 决定"= **推断**（缺：`cfg` 分支本身的字节。
可查办法：找 `GraphwarAdSwitch` 里被 `cfg` 掉的那半段——它连字符串都不会留下，
所以只能从"两串等长、其余代码同构"反推）。

`owned` 位（`[obj+0x18]`）的语义由 UI 文案坐实：
`player_screen.tscn` 里 `UnlockButton` 上方的 Label 写着 **`"Unlocking skins also removes all ads"`**。
⇒ `owned == true` ＝ 已购买 `graphwar_support` ＝ 广告关闭。等级：**观测**。

### 2.4 `backend` 不是广告模块 —— 它是联机层

`docs/02` §14.4 把 `graphwar_backend`(6) 和 `graphwar_ad_switch`(7) 一起归到"广告 / 后端"堆里，
**那一格是错的**（名字里的 "backend" 指的是网络后端）。证据：

1. `GraphwarBackend` 的方法名（符号表，Android，15 条 `FUNC`）：

   ```
   new  process  button_pressed  setup_level
   start_server  stop_server  start_client_to_local_server  start_client_to_remote_server
   start_client_to_replay  get_next_clip_checkpoint
   start_lan_broadcaster  stop_lan_broadcaster
   start_server_broker_handler  stop_server_broker_handler
   subscribe_to_event  subscribe_to_event_with_id  resubscribe_to_events
   ```

2. 它引用的字面量是 `"Graphwar GameServer failed to start, cannot connect client."`（`0x19ca23`）、
   `"Incompatible version."`、`"Cannot set non-blocking"`、`VersionInfoEvent` / `room_name` 等。
3. **它没有注册成 Godot 类**：`GraphwarBackend` 这个类名只出现在 `.strtab`/`.symtab`（偏移 ≥ `0xb21eb5`），
   **`.rodata` 里没有**；也没有 `ImplementsGodotApi::__register_methods` / `GodotClass::class_id` /
   `__register_constants` 这三件套。对照 `GraphwarAdSwitch` 三件套齐全、且 `.rodata` 有类名。
   ⇒ 它是**纯 Rust 内部结构**，GDScript 见不到、`main.tscn` 里也没有它的节点。

**对复原的意义**：`HANDOVER` §0 的第 2 项交付（"联机网络层没做——客户端不会连服务器"）
在官方包里有**完整且带名字的实现**。Android 那份未 strip 的 `.so` 可以直接当骨架：
15 个方法名 + 每个方法的字节数 + 调用点，足以把 `src/broker.rs` 之外的编排层补齐。
（本次**没有**反编译它们，只做了符号级定位；这是给下一个 agent 的线索，不是结论。）

### 2.5 `purchase_confirmed` 的"2 处"是符号表造成的

```
Android  purchase_confirmed:  off=0x19aa1f (rodata)  +  off=0xc516d1 (strtab，符号名)
Linux    purchase_confirmed:  off=0x1bd854 附近那一段 (rodata)   ← 只有 1 处
```

`0xc516d1` 落在 `.strtab` 区间（`.strtab` 起 `0xb21eb5`，长 `0x326ac3`），内容就是
`_ZN12graphwarrust18graphwar_root_node16GraphwarRootNode18purchase_confirmed17hea9cd18c8369549cE`。
**同一条规律**也解释了 `purchase_denied`（A=2/L=1）、`request_permissions_result`（A=3/L=1）。
⇒ 结论：**代码层面两边一致，差的只是 Linux 那份被 strip 了**。等级：**逐位验证**（对整份文件数字节）。

### 2.6 `unlock_skins` / `purchase_graphwar_support` 解锁的是什么、存哪

**商品**：只有一个，id = **`graphwar_support`**，`android_iap.gd` 里 `const ITEM_ACKNOWLEDGED = ["graphwar_support"]`，
`PurchaseState.PURCHASED` 且未 ack 时先 `acknowledgePurchase(token)` 再 `purchase_confirmed.emit()`。
⇒ 一次性非消耗型内购（就是"支持作者"）。等级：**观测**（.gd 原文）。

**`unlock_skins` 是一个按钮的 `input_text`**，不是函数：

```
player_screen.tscn:425   [node name="UnlockButton" type="GraphwarButton" …]
                         input_text = "unlock_skins"
                         text = "Unlock"
（同一块 SkinBlock 里的 Label）text = "Unlocking skins also removes all ads"
```

Rust 侧在 `GraphwarRootNode::process_button_pressed`（`0x3a6d48`，函数体 ≈ 5.7 KB）里：

| 位置 | 内容 |
|---|---|
| `+0x1148`（`0x3a7e90`） | 引用字面量 `unlock_skins`（12 字节，`0x19abb7`） |
| `+0x11fc`（`0x3a7f44`） | 引用 `purchase_graphwar_support`（25 字节，`0x19abc3`＝前者的紧邻），并做 `IAP_holder.call_deferred("purchase_graphwar_support")` |

⚠️ 这两个字面量在 `.rodata` 里**首尾相接**，`0x19abb7+12 == 0x19abc3`，所以编译器复用了同一个基址指针。
**这个 5.7 KB 函数的 `match` 臂边界本次没有还原**（观测到了地址与调用，没有还原控制流）。
缺的证据：对该函数做一次完整反编译 / 或用寄存器值在运行期打点。

**`GraphwarPlayerConfig`（`0x392964` / `0x392248`）到底改了什么** —— 六个字段槽的语义是逐指令读出来的：

| 槽 | 含义 | `lock()`（未购买） | `unlock()`（已购买） |
|---|---|---|---|
| `+0x18` | 生效的 **skin** | 写入 `"yellow"`（6 B，字面量就地拼出） | ← `+0x60` 的克隆 |
| `+0x30` | 生效的 **face** | 写入 `"regular_eyes"`（12 B） | ← `+0x78` 的克隆 |
| `+0x48` | 生效的 **hat** | 写入 `"soldier_helmet"`（14 B） | ← `+0x90` 的克隆 |
| `+0x60/+0x78/+0x90` | 玩家**自己选**的那套 | 不动 | 被读出来 |
| `+0xa8` | 计数器 | 不动 | `+= 1` |
| `+0xac` | **locked 标志（bool）** | `= 0` | `= 1` |

⇒ **未购买 = 强制默认外观（黄皮肤 / 普通眼睛 / 钢盔）且不能选；购买后才把玩家自选的那套拷进生效槽。**
`GraphwarPlayerScreenControl` 的三个状态函数各 324 B，动手的只有两个节点
（`get_node_by_name` 的字面量：`"SkinBlock"` 9 B @ `0x19df53`、`"PendingBlock"` 12 B @ `0x19df5c`）：

| 函数 | `SkinBlock.set_visible` | `PendingBlock.set_visible` |
|---|---|---|
| `unlock()` `0x450600` | `false`（`0x450710` `mov w1,wzr`） | `false`（`0x450714`） |
| `lock()` `0x4504bc` | `true`（`0x450550` `mov w1,#1`） | `false`（`0x4505d0`） |
| `pending()` `0x450744` | `false`（`0x4507d8`） | 接着取 `PendingBlock`（`0x450814`），可见值在同一路径上 |

等级：**观测**（`set_visible` 的立即数与节点名字面量都给了地址；`pending()` 第二次调用的
`w1` 未逐条核到，标 **推断**）。

**存哪**：`GraphwarPlayerConfig::load`（`0x3917d4`）与 `save`（`0x392388`）都引用
**`user://player.config`**（`0x19a722`）；`save` 里另有 `"true"`/`"false"` 两个字面量（`0x1945c4`/`0x19a638`），
即 `+0xac` 那个 bool 是以文本 `true`/`false` 落盘的。
⇒ **不走 `player.config` 之外的玩家配置，也不写 `project.godot`**。等级：**观测**（文件路径字面量 + `.gd` 无对应写盘）
／**推断**（具体键名未解析：缺一次运行期的 `user://player.config` 实物，两平台都需要跑起来才有）。

### 2.7 是否影响玩法 —— 不影响内核

用未 strip 的符号表把"skin/face/hat/graphics"相关的 `FUNC` 按模块归堆：

| 模块 | 命中条数 |
|---|---|
| `graphwar_graphics_manager` | 14 |
| `graphwar_event` | 12（`SkinGraphicsInfoEvent` / `SkinGraphicsRequestEvent` / `*Face*` / `*Hat*` 上下线） |
| `graphwar_player_screen_control` | 6 |
| `graphwar_texture_loader` | 3（`GraphwarTextureLoader::get_skin_texture`） |
| `graphwar_player_config` | 3 |
| `graphwar_game_state_manager` | 1 |

**采样器 / 掩膜 / `shot_calculator` / `plane` / 命中 / `life_manager` / `target_manager` / `turn_manager`
里一条都没有。** ⇒ 皮肤是纯外观：它上线只是为了**让别人也看见**（`SkinGraphicsRequestEvent`→
`SkinGraphicsInfoEvent`，`docs/96` 已覆盖这组事件），不参与任何几何或判决计算。

⇒ **可以明确写：广告 / 内购链路不影响内核复原。** 等级：**观测**（符号归属 + 交叉引用计数）；
"不影响"这一句是**推断**，缺的是"玩家 A 换皮肤后玩家 B 的命中结果不变"的运行期对照
（要做得多客户端同时在跑，属 `docs/06` §17.2 的 B 类退化口径）。

### 2.8 广告：**怎么才会出现**（用户实测问题：Android 上看不到广告）

> ⚠️ 先纠正一条流传的说法：**"`.so` 里写的是 Google 测试广告位"只对 Linux 那份成立。**
>
> | 包 | `.rodata` 里的广告位 | 性质 |
> |---|---|---|
> | **Android** `libgraphwarrust.so` | `ca-app-pub-2005941169362714/5433871375` @ `0x19dd6c` | **发布者的真实广告位**（publisher id `2005941169362714` 与 `AndroidManifest.xml` 里 `com.google.android.gms.ads.APPLICATION_ID = ca-app-pub-2005941169362714~2616868867` **是同一个账号**） |
> | Linux `libgraphwarrust.so` | `ca-app-pub-3940256099942544/6300978111` @ `0x1c43ff` | Google 官方**测试**横幅位 |
>
> 所以"装的是官方 Android 包却看不到广告"**不是**"用了测试位"造成的。等级：**观测**（两串都在，
> manifest 的 App ID 用 `strings -el` 从 base.apk 的二进制 XML 里取出）。

#### ① `are_ads_enabled()` 的判据 = *只*看内购有没有买

`GraphwarRootNode` 三个处理函数里对同一个对象的同一个字段的读写，是全二进制里**唯一**的写点：

| 位置（Android） | 动作 |
|---|---|
| `purchase_confirmed` `0x3a6558`–`0x3a658c` | `bind_mut(<GraphwarAdSwitch>)` → `strb w10(=1), [obj+0x18]` |
| `purchase_denied` `0x3a6180`–`0x3a61b0` | `bind_mut(<同一对象>)` → `strb wzr(=0), [obj+0x18]` |
| `are_ads_enabled()`（内联体 `0x377cf4`） | `ldrb w22,[obj+0x18]` → `eor w8,w22,#1` → 返回。**= `!owned`** |

⇒ **判据里没有平台判断、没有地区、没有同意状态、没有 `player.config` 的读取**。
`owned` 只有两个来源：IAP 的 `purchase_confirmed` / `purchase_denied`。
等级：**观测**（三条指令地址都给了）。
⇒ 推论（**推断**，缺"启动时是否重新查单"的运行期证据）：因为 `android_iap.gd` 在
`_on_connected()` 里会 `queryPurchases("inapp")`，买过的人每次启动都会再收到一次
`purchase_confirmed`（`purchases_list` 为空则发 `purchase_denied`），
**这就是"购买状态跨启动"的机制，而不是把 flag 落盘**——`player.config` 里存的是外观，不是广告开关。

#### ② `get_ad_unit_id()` 返回 **写死的常量**

内联体 `0x36f3d0` 只有一句有效操作：`GString::from_str(<字面量>, 38)`（`0x36f444`→`0x19dd6c`）。
**不从服务端 `game_address` 那套配置读，不从 `player.config` 读，不按地区切换。**
等级：**观测**。

#### ③ 谁调用 `create_ad_view` / `destroy_ad_view`

三处，**都不是定时器、也不是对局结束事件**：

| 调用者 | 动作 | 证据 |
|---|---|---|
| `Ad_holder.gd::_ready()` | `MobileAds.initialize()` + `create_ad_view()` | .gd 原文 |
| `GraphwarRootNode::show_screen` `0x3a5ea8` | 目标屏**变为可见** → `Ad_holder.call_deferred("create_ad_view")`（`0x3a6038`）；**变为不可见** → `call_deferred("destroy_ad_view")`（`0x3a60dc`） | 观测（`CanvasItem::is_visible` @`0x3a5ff0`/`0x3a6094`，`cbz/cbnz w0` 就是两个分支的条件） |
| `purchase_confirmed` `0x3a65d8` | `call_deferred("destroy_ad_view")`（买完立刻撤广告） | 观测 |

而 `Ad_holder.gd::create_ad_view()` 自己再判一次：`if ad_switch.are_ads_enabled()` → 才
`AdView.new(unit_id, AdSize.FULL_BANNER, AdPosition.Values.BOTTOM)` + `load_ad(AdRequest)`。

**广告位在哪些界面**（`.tscn` 里有 `AdContainer` 节点的，共 12 个）：

```
main_menu  campaign  GameSetup  lan_lobby  player_screen  replay_lobby
add_bot_pop_up  confirm_exit_pop_up  connecting_pop_up  disconnected_pop_up
join_ip_pop_up  join_level_pop_up
```

**`play_screen` 里一个都没有** ⇒ **对局中不展示广告**，只在菜单/大厅/玩家页/弹窗里出。
等级：**观测**（`.tscn` 逐个 grep；Android 的 `.scn` 是同一份资产，§3.1 已证逐字节相同）。

#### ④ `AdContainer` 的高度：初始 0，但**这不是"看不见"的原因**

* `Ad_holder.gd`：`var ad_size = Vector2i(0, 0)`；
* `AdContainer.gd`：`custom_minimum_size = ad_holder.ad_size`（`visibility_changed` 与
  `ad_loaded` 两个信号都触发重算）；
* 真正撑开是在 `on_ad_loaded` 里：`ad_size = Vector2i(ad_view.get_width_in_pixels(), ad_view.get_height_in_pixels())`。

⚠️ 但横幅是**原生 view**（`AdView.new(..., AdPosition.Values.BOTTOM)` 由 AdMob 插件在 Android
侧叠一层），**不经过 Godot 的节点渲染**。所以：

* 高度 0 **不会**让已经加载出来的横幅消失；
* 反过来，如果 `AdContainer` 撑开了空白而底下没图 ⇒ **广告请求发出去了但没有填充**；
* 如果连空白都没有 ⇒ **`create_ad_view()` 根本没被调用**（或 `are_ads_enabled()` 为假）。

等级：**观测**（脚本原文 + 调用点）；"原生 view 与 Godot 布局互不影响"= **推断**
（缺：一次真机上 `dumpsys window`/截图对照。这条判据对定位问题很关键，建议真机核一次）。

#### ⑤ UMP 同意流程：游戏**从不调用**

* 105 个游戏脚本里 `consent` / `Ump` / `UserMessagingPlatform` / `request_consent` **命中 0 处**；
* `addons/admob/sample/Ump.gd|tscn`、`addons/admob/src/ump/api/{ConsentForm,ConsentInformation,UserMessagingPlatform}.gd`
  是**插件自带的示例与 API**，游戏侧没有任何引用；
* `Ad_holder.gd` 的 `_ready()` 只有 `MobileAds.initialize()` 一句，之后直接 `create_ad_view()`。

⇒ **游戏没走 UMP**。等级：**观测**（全量 grep 为 0）。
⇒ 由此产生的**推断**（缺真机验证）：在 EEA/英国，Google 对没有认证 CMP 的流量可以拒绝填充；
在中国大陆，Google 移动广告服务本身不可达。两种都会表现为"代码跑了但没填充"。

#### ⑥ 给用户的复现清单（代码层已排除的 vs 还需真机确认的）

**代码层能确定的（不用试）**：
1. 广告的开关**只有** `graphwar_support` 有没有买；
2. 广告位 id 是发布者真实位，**不是测试位**；
3. 广告**只在 12 个界面出现，对局中不会有**；
4. 游戏**不会**弹 UMP 同意框；
5. 请求链是 `_ready()`/`show_screen` → `create_ad_view()` → `AdView.load_ad()`，中间的
   每一步都有日志（`root.log("create_ad_view")` / `"_on_ad_loaded"` /
   `"_on_ad_failed_to_load: " + 错误信息`）。

**要真机 + 真网络才能定论的（按这个顺序查）**：

| # | 查什么 | 怎么查 | 判读 |
|---|---|---|---|
| 1 | `create_ad_view` 有没有跑 | `adb logcat` 看 Google 的 tag：`Ads`、`MobileAds`、`AdsInfo`、`AdsService` | 有 `Ad failed to load : 3` ⇒ **无填充**（账号/广告位/地区问题，不是代码问题） |
| 2 | `MobileAds.initialize()` 成不成 | logcat 里搜 `MobileAds` / `APPLICATION_ID` | 报 App ID 缺失或初始化失败 ⇒ manifest/SDK 问题 |
| 3 | 有没有"广告位撑开但空白" | 进 `main_menu` / `player_screen` 截图 | 底部多一条空白 ⇒ 请求已发、无填充（同上） |
| 4 | 网络能不能到 Google 广告服 | 设备上 `curl -I https://googleads.g.doubleclick.net` 或看 logcat 的 DNS/超时 | 大陆网络 ⇒ **必然无填充，且不是 bug** |
| 5 | 是不是已经买过 | Play 商店账号"已购买"列表 / 重装后看 | 买过 ⇒ `owned=true`，广告**按设计**不出现 |
| 6 | 是不是测试设备 | AdMob 后台的 test device 列表 | 只影响"能拿到测试广告"，不影响线上填充 |

> **一句话给用户**：广告在**主菜单/大厅/玩家页**底部的原生横幅位；
> 它的唯一开关是"有没有买 `graphwar_support`"；看不到只可能是
> **①已经买过 ②请求无填充 ③设备连不上 Google 广告服务**——代码里没有第四种可能。

### 2.9 本节的未知

1. `purchase_pending` 在 Rust 侧的处理点（只看到 `GraphwarPlayerScreenControl::pending` 存在，没找到调用者）。
2. `request_permissions_result` 的语义（为什么 `ready()` 把 `on_request_permissions_result` 连到
   `self.base().get_tree()` 上——SceneTree 没有这个内置信号；**推断**是连到某个持有该信号的单例上，
   缺一次反编译确认）。
3. `process_button_pressed` 里 `unlock_skins` 分支的完整控制流。
4. `user://player.config` 的实际键名与格式。

---

## 3. 平台差异对照

### 3.1 游戏数据：三平台逐字节相同（这是最强的一条）

| 对比 | 手段 | 结果 |
|---|---|---|
| Linux `Graphwar.pck` vs Windows 内嵌 PCK | 解析两边 PCK 索引（692 条），比 path / size / **md5** | **0 个不同、0 个单边** |
| Linux `Graphwar.pck` vs Android `split_assetPackInstallTime.apk` | 452 个非 `.godot/` 资产逐个 md5 | **452 全部相同、0 个不同** |
| `project.binary` | 三份都是 1346 B，md5 `8815cde6bcd8dbb7a6a0dd424d2afa41` | 相同 |
| `Graphwar.gdextension` | 三份都是 1491 B，md5 `54ada740ea15a202b23e54946cb2c2df` | 相同 |

⇒ **三份包的界面、脚本、场景、主题、关卡数据是同一份字节。** 等级：**逐位验证**。

### 3.2 字面量层：只找到一处平台条件常量

把三份包的**只读数据节**（Linux/Android `.rodata`、Windows `.rdata`）里的可打印串取集合做差
（**只取数据节**——Android 的 `.strtab` 有 10 万条符号名，混进来会假报 27 000 条差异）：

| 差集 | 条数 | 说明 |
|---|---|---|
| A − L | 466 | 主要是 `serde_json` / `regex` / `rand` 内部的错误串，随编译单元不同而不同 |
| L − A | 716 | 同上（两边各自的 inlining 组合不同） |
| L − W | 1170 | 含 `gimli`/`addr2line`/`DW_FORM_*` 等**调试回溯**串（Windows 独有，见 §3.5） |
| W − L | 3152 | 同上 + `clap_builder` 的 `debug_assert` 符号名等 |

**逐条看下来，属于"玩法参数"的差异是 0 条**；唯一一处**确定是平台条件**的是 AdMob 广告位 id（§2.3）。
等级：**观测**（集合差集）；"没有别的"= **推断**（差集只能证明"字面量不同"，
证明不了"代码路径相同"——见 §3.3）。

⚠️ 反向提醒：`room-auto` 只在 Android/Windows 有、Linux 没有；`graphwar_ad_switch.rs:LINE` 这类
panic 位置串两边条数相同（95 = 95）。这些**都不是**玩法分叉，见 §3.6。

### 3.3 浮点/数学常量：**比不了，别假装比过**

本任务**没有**做"常量表逐字节相同"的结论，原因如下（这条要写给后面的人）：

* 跨 ISA 的**代码**无法逐字节比；只有**数据**能比。
* 但同一个 `f64` 字面量在三个 target 上的落点并不稳定：可能进 `.rodata`、可能被拆成立即数
  （aarch64 的 `movz/movk`）、可能被常量合并去重。实测计数（`.rodata`/`.data` 内 8 字节窗口）：

  | 常量 | Android | Linux | Windows |
  |---|---|---|---|
  | `0.01` (`0x3f847ae147ae147b`) | 2 | 2 | 1 |
  | `0.001` (`0x3f50624dd2f1a9fc`) | 1 | 1 | 2 |
  | `200.0` | 0 | 1 | 3 |
  | `25.0` | 0 | 1 | 2 |
  | `15.0` | 1 | 1 | 4 |

  **这些差别不是"常量不同"，而是"同一常量被物化的次数不同"**（例如 Android 把 `200.0` 直接
  编成 `fmov` 立即数，就不在 `.rodata` 里）。**用计数差推结论是错的**，所以本文不推。

* 真正能钉住数学一致性的三条，本次只有前两条可做：
  1. **资产逐字节相同**（§3.1，已做，成立）；
  2. **源码同一份**：Rust 把 `src/<file>.rs:<line>` 编进 panic 位置。三份包的这套串
     **Android 95 / Linux 95，两个集合完全相同（95 ⊂ 95，双向无差）**；
     Windows 是 95 + 12 条额外（见 §3.6）。⇒ **至少这 95 个 panic 点所在的行，
     三平台是同一份源码**。等级：**逐位验证**（集合相等）。
  3. 采样器/掩膜的**输出**逐点相同 —— **未做**。要做得能在 Android/Windows 上真跑起来
     （设备 / wine + Godot-for-Windows），本机不具备。

### 3.4 壳层额度（`project.binary`）—— 三平台完全一致

`project.binary` 是 Godot 的二进制 ConfigFile（`ECFG`，格式版本 21）。三份包的这份文件**逐字节相同**，
共 21 个键（全量）：

| 键 | 值 | 只影响 |
|---|---|---|
| `display/window/size/viewport_width` / `_height` | `1600` / `900` | 壳层 |
| `display/window/size/mode` | `2`（MAXIMIZED） | 壳层 |
| `display/window/handheld/orientation` | `4`（SENSOR_LANDSCAPE） | 壳层（仅手持设备生效） |
| `input_devices/pointing/emulate_touch_from_mouse` | `true` | 壳层（桌面上的触摸模拟） |
| `rendering/renderer/rendering_method.mobile` | `gl_compatibility` | 渲染后端 |
| `rendering/textures/vram_compression/import_etc2_astc` | `true` | 资源导入 |
| `rendering/anti_aliasing/quality/msaa_2d` | `2` | 渲染 |
| `rendering/textures/canvas_textures/default_texture_filter` | `2` | 渲染 |
| `rendering/environment/defaults/default_clear_color` + `application/boot_splash/bg_color` | `Color(0.0549,0.1451,0.1098,1)`（深绿） | 渲染 |
| `application/config/{name,description,version,icon,features}` | `Graphwar` / … / `2.0.60` / `res://basic_soldier.png` / `[4.7, Mobile]` | 元信息 |
| `application/run/main_scene` | `res://main.tscn` | 壳层 |
| `application/boot_splash/show_image` | `false` | 壳层 |
| `animation/compatibility/default_parent_skeleton_in_mesh_instance_3d` | `true` | 无关 |
| `filesystem/import/blender/enabled` | `false` | 无关 |
| `editor_plugins/enabled` | `[admob/plugin.cfg, android_IAPP/plugin.cfg]` | 编辑器 |

⚠️ **`application/config/features` 里只有 `[4.7, Mobile]`，桌面和 Android 一模一样**；
`display/window/size/mode = 2` 在 Android 上也不是全屏 —— 说明 Android 的屏幕参数是
**导出预设（export preset）里覆盖的，不在 `project.binary` 里**。
⇒ 想找"Android 专属的壳层额度"，得去看 APK 的 `AndroidManifest.xml`，`project.binary` 里没有。
等级：**观测**（21 个键全部解出，见 `.agent-work/android/ecfg_*.txt`；文件不入库）。

### 3.5 Windows 为什么 41 MB

**观察到的构成**（`objdump -h` + PE 头字段）：

| 段 / 区 | Windows DLL | Linux `.so` | 倍数 |
|---|---|---|---|
| 代码 `.text` | **21 713 080** | 6 107 716 | **3.56×** |
| 只读数据 | `.rdata` 2 659 320 | `.rodata` 1 129 484 | 2.35× |
| 展开/异常表 | `.pdata` 586 236 + `.xdata` 1 958 788 | `.eh_frame` 723 272 + `.gcc_except_table` 1 084 068 | 1.4× |
| 其余段 | 155 963 | 其余 ≈ 2.0 M | — |
| **节合计** | **27 072 787** | **10 225 183** | |
| **尾部 COFF 符号表** | **14 186 410**（`NumberOfSymbols=147444`，18 B/条 = 2 654 000 + 字符串表 11 532 693） | 0（已 strip） | |
| **文件** | **41 259 197** | 10 227 896 | 4.03× |

⇒ **41 MB 里有 14.19 MB（34%）是符号表**，`-Wl,--strip-all` 一下就没了。
剩下的 27 MB 是"真的更多机器码"。

**为什么代码多 3.56 倍** —— 两条观测，一条推断：

1. **函数个数差 3.6 倍**（观测）：Windows `.pdata` 的 `RUNTIME_FUNCTION` 记录数
   `586236 / 12 = 48 853`；Linux `.eh_frame_hdr` 的 FDE 计数 = **13 430**；
   Android `.symtab` 的 `FUNC` 条数 = 18 549。⇒ Windows 保留了约 4.9 万个函数体。
2. **Windows 那份开着溢出检查与调试断言**（观测）：整串
   `attempt to add with overflow` / `attempt to divide with overflow` /
   `attempt to multiply with overflow` / `attempt to negate with overflow`
   只出现在 Windows 的 `.rdata` 里，Linux/Android **一个都没有**（`grep -ac` 全 0）。
   ⇒ 这份 DLL 不是普通 `--release`：`overflow-checks = true`（并且很可能 `debug-assertions = true`），
   每一次整数运算都带一条 panic 分支——这正是代码膨胀最典型的来源。
   顺带解释了为什么它带 `gimli`/`addr2line`/`DW_FORM_*` 这类**符号化回溯**机器。
3. **链接期没有做等价合并/GC**（推断）：Windows 独有的模块串（§3.6）说明未引用的代码没被丢掉，
   而 mingw 的默认链接不带 `--gc-sections`。缺的证据：原版 `Cargo.toml` 的
   `[profile.release]` 与 `.cargo/config.toml`（我们没有源码，只能从产物反推）。

**对我们出 Windows 包的直接影响**：
- 用普通 `cargo build --release`（带 LTO、不带 overflow-checks）出包，**不会**是 41 MB；
  按 Linux 的比例估算应在 **8–12 MB** 量级。**
- 反过来说：**官方 Windows 包的性能不是官方 Linux 包的性能**（每次整数运算多一条分支）。
  如果要做跨平台行为对拍，得注意这一点（溢出在 release 下是回绕、在这份包里是 panic）。

### 3.6 Windows 独有、Linux/Android 没有的东西（别当成玩法分叉）

`src:line` 串的差集里，Windows 多出 12 条，全部来自 4 个 Linux/Android **完全不存在**的模块：

```
src/room_server_room_provider.rs:48,52,53,151
src/server_broker_http_handler.rs
src/server_broker_room_request_handler.rs:72,90,109,125
src/server_broker_room_provider_handler.rs:51
src/server_broker_request_connection.rs:47
```

在整份文件里 `grep` 的原始计数：`room_server_room_provider` A=0 / L=0 / **W=5**；
`server_broker_room_request_handler` 0/0/5；`server_broker_http_handler` 0/0/2。
（另外 `room-auto` 是 A=1 / L=0 / W=1——它在 `graphwar_server_broker_handler` 附近，与上面那 4 个模块无关。）

**两种解释，本次没有定论**：

* **(a) 死代码没被丢掉**（推断，我更倾向这条）：这些是 broker/房间服务的代码，从 Godot 的入口
  不可达；Linux/Android 用 LTO + 链接期 GC 把它删了，Windows 的 mingw 链接默认不删。
  支持证据：函数个数 3.6×、COFF 符号表在、并且 §3.5 那批 overflow 检查把 panic 路径全部保留。
* **(b) Windows 那份源码确实更长**（未知）：如果它真是另一个提交，那就与"三平台同版本 2.0.60"
  冲突——但 `project.binary` / PCK / `.gdextension` 又逐字节相同。

**缺的证据**（谁能补）：拿到 Windows 那份的 `main`/`gdext_rust_init` 调用图（符号表在，能做），
看 `server_broker_room_*` 是否被任何从 `gdext_rust_init` 可达的函数引用。
⇒ **可以确定的是：这 4 个模块不影响对局**（它们是 broker/房间服务端，不是对局逻辑），
所以无论 (a) 还是 (b)，"跨平台玩法一致"这条都不受影响。

### 3.7 平台差异汇总表

| 维度 | 有差异吗 | 影响玩法吗 | 等级 |
|---|---|---|---|
| 游戏资产（PCK/脚本/场景/主题/关卡） | **无**（逐字节相同） | 否 | 逐位验证 |
| `project.binary`（壳层额度） | **无** | 否 | 逐位验证 |
| `Graphwar.gdextension` | **无** | 否 | 逐位验证 |
| AdMob 广告位 id | **有**（Android 真实 / Linux 测试位） | 否 | 观测 |
| 采样器 / 掩膜 / 命中的数学 | 未测（跨 ISA 不可比） | ? | 未知（见 §3.3） |
| `cfg(target_os)` 触屏/朝向分支 | **没找到** | — | 观测（字面量差集） + 推断 |
| 编译配置（overflow-checks / LTO / 符号表） | **有**（仅 Windows） | **否**（除非真的溢出） | 观测 |
| Windows 独有的 broker 房间模块 | 有（4 个模块） | 否 | 观测 |

---

## 4. 跨平台构建说明

### 4.1 原版 `.gdextension`（原样，三平台同一份）

从 `Graphwar.pck` 里取出（1491 B，md5 `54ada740ea15a202b23e54946cb2c2df`）：

```ini
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.1

[libraries]
linux.debug.x86_64   = "res://../rust/target/debug/libgraphwarrust.so"
linux.release.x86_64 = "res://../rust/target/x86_64-unknown-linux-gnu/release/libgraphwarrust.so"
windows.debug.x86_64   = "res://../rust/target/x86_64-pc-windows-gnu/debug/graphwarrust.dll"
windows.release.x86_64 = "res://../rust/target/x86_64-pc-windows-gnu/release/graphwarrust.dll"
macos.debug   = "res://../rust/target/debug/libgraphwarrust.dylib"
macos.release = "res://../rust/target/release/libgraphwarrust.dylib"
ios.debug   = "res://../rust/target/aarch64-apple-ios/debug/libgraphwarrust.dylib"
ios.release = "res://../rust/target/aarch64-apple-ios/release/libgraphwarrust.dylib"
android.debug.x86_64   = "res://../rust/target/x86_64-linux-android/debug/libgraphwarrust.so"
android.release.x86_64 = "res://../rust/target/x86_64-linux-android/release/libgraphwarrust.so"
android.debug.x86_32   = "res://../rust/target/i686-linux-android/debug/libgraphwarrust.so"
android.release.x86_32 = "res://../rust/target/i686-linux-android/release/libgraphwarrust.so"
android.debug.arm64    = "res://../rust/target/aarch64-linux-android/debug/libgraphwarrust.so"
android.release.arm64  = "res://../rust/target/aarch64-linux-android/release/libgraphwarrust.so"
web.debug.wasm32   = "res://../rust/target/wasm32-unknown-emscripten/debug/graphwarrust.wasm"
web.release.wasm32 = "res://../rust/target/wasm32-unknown-emscripten/release/graphwarrust.wasm"
```

三条能直接抄的信息（等级：观测）：

1. **target 三元组**：Linux `x86_64-unknown-linux-gnu`；Windows **`x86_64-pc-windows-gnu`**
   （就是 mingw，不是 MSVC —— 与 `Win/` 里那三个 `libgcc_s_seh-1.dll`/`libstdc++-6.dll`/
   `libwinpthread-1.dll` 对得上）；macOS/iOS 走默认/`aarch64-apple-ios`；
   Android 提供 `x86_64` / `x86_32` / **`arm64`**（没有 armv7）；web 是
   `wasm32-unknown-emscripten`。
2. **路径是"构建产物路径"，不是"发布路径"**：`res://../rust/target/...` 是作者的开发树。
   但**发布目录里库就躺在可执行文件旁边**（`gw2/Graphwar.x86_64` 与 `gw2/libgraphwarrust.so` 同级，
   `start_graphwar.sh` 也是这么启动的；APK 里则是 `lib/arm64-v8a/libgraphwarrust.so`）。
   ⇒ 导出器**按 basename 把库拷到导出目录 / `lib/<abi>/`**。
   （等级：观测——发布目录结构就是这样；导出器内部行为是**推断**。）
3. `compatibility_minimum = 4.1`，`entry_symbol = "gdext_rust_init"`（godot-rust 的默认入口）。

### 4.2 我们的 `client/gw2rebuild.gdextension`（已补 windows / macos / android）

见本仓库 `client/gw2rebuild.gdextension`。补的规则与上面那张表一一对应，只把
`graphwarrust` 换成我们的 `gw2_rebuild`、把开发树路径换成 `res://` 下我们自己的产物位置：

| 平台 | 条目 | 值 |
|---|---|---|
| Linux | `linux.debug.x86_64` / `linux.release.x86_64` | `res://libgw2_rebuild.so`（原有，未改） |
| Windows | `windows.debug.x86_64` / `windows.release.x86_64` | `res://gw2_rebuild.dll` |
| macOS | `macos.debug` / `macos.release` | `res://libgw2_rebuild.dylib` |
| Android | `android.debug.arm64` / `android.release.arm64` | `res://android/arm64-v8a/libgw2_rebuild.so` |

⚠️ Android 的条目不写 `res://libgw2_rebuild.so`：**同一个 `res://` 目录下同时放 Linux 的 `.so`
和 Android 的 `.so` 会互相覆盖**（同名）。所以 Android 的产物要先按 ABI 放进子目录，
再由导出器拷成 `lib/arm64-v8a/libgw2_rebuild.so`。这也是原版为什么用
`res://../rust/target/aarch64-linux-android/release/` 这种分开的路径。

### 4.3 每个平台需要什么、产出什么、放哪

| 平台 | target | 工具链 | 产出 | 放哪 |
|---|---|---|---|---|
| Linux | `x86_64-unknown-linux-gnu` | 系统 gcc + `rustup target`（本机已有） | `target/release/libgw2_rebuild.so` | `client/libgw2_rebuild.so`（**gitignore**：`client/*.so`） |
| Windows | `x86_64-pc-windows-gnu` | `rustup target add x86_64-pc-windows-gnu` + **mingw-w64**（`x86_64-w64-mingw32-gcc`） | `target/x86_64-pc-windows-gnu/release/gw2_rebuild.dll` | `client/gw2_rebuild.dll`（需扩 `.gitignore`，见 §4.4） |
| macOS | 本机即 `aarch64-apple-darwin` / `x86_64-apple-darwin` | **必须在 macOS 上编**（Xcode CLT）；通用包用 `lipo` | `target/release/libgw2_rebuild.dylib` | `client/libgw2_rebuild.dylib` |
| Android arm64 | `aarch64-linux-android` | **Android NDK r27+**（原版就是 NDK r27-beta2 打的：`.note.android.ident` + `libc++_shared.so`） | `target/aarch64-linux-android/release/libgw2_rebuild.so` | `client/android/arm64-v8a/libgw2_rebuild.so` |

### 4.4 命令（**本次没有执行**，本机没有 mingw / NDK，装它们不在本任务范围）

```bash
# --- Linux（本机可跑，已有产物）
export CARGO_HOME=$PWD/.cargo-home
export CARGO_TARGET_DIR=$PWD/.cargo-home/target
cargo build --release --offline
cp "$CARGO_TARGET_DIR/release/libgw2_rebuild.so" client/
godot --headless --path client --import        # ← 见 HANDOVER：不 import 会静默不加载

# --- Windows（需要 mingw-w64）
rustup target add x86_64-pc-windows-gnu
# .cargo/config.toml:
#   [target.x86_64-pc-windows-gnu]
#   linker = "x86_64-w64-mingw32-gcc"
#   rustflags = ["-C", "target-feature=+crt-static"]   # 否则要跟着发那三个 mingw 运行库
cargo build --release --offline --target x86_64-pc-windows-gnu

# --- Android arm64（需要 NDK）
rustup target add aarch64-linux-android
# 二选一：
#   A. cargo-ndk（会自己找 NDK 并填 linker/ar）
#      cargo ndk -t arm64-v8a -o client/android build --release --offline
#   B. 手工（.cargo/config.toml）
#      [target.aarch64-linux-android]
#      linker = "$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android29-clang"
#      rustflags = ["-C", "link-arg=-Wl,-soname,libgw2_rebuild.so"]
cargo build --release --offline --target aarch64-linux-android

# --- macOS（只能在 macOS 上）
cargo build --release --offline
# 通用二进制：
lipo -create -output libgw2_rebuild.dylib \
     target/aarch64-apple-darwin/release/libgw2_rebuild.dylib \
     target/x86_64-apple-darwin/release/libgw2_rebuild.dylib
```

**`.gitignore` 还差两条**：现在只挡了 `client/*.so`，没挡 `*.dll` / `*.dylib`。
本次已补（见提交），避免哪天顺手 `git add` 进去了（`AGENTS.md` 硬规矩 3）。

### 4.5 Windows 导出模板：现状

* **本机没有任何导出模板**：找不到 `*.tpz`，也没有 `export_templates/` 目录
  （`godot-dist/` 里只有我们自己编的 `libgw2_rebuild.so` 和 `out.pck`）。
* **`Win/` 里没有 `Graphwar.pck`**：PCK 是**内嵌在 `Graphwar.exe` 尾部**的
  （`GDPC` 头在 `0x67fd600` = 109 043 200，正好在 exe 的最后一个字节结束；尾部还有
  `u64 pck_size + "GDPC"` 的"内嵌 pck"标记）。解析出的索引与 Linux `Graphwar.pck` **完全一致**（§3.1）。
* **`Graphwar.exe` 能不能当模板用？** 结构上它 = 导出模板 + 内嵌 PCK + 尾部标记，所以理论上
  `head -c 109043200 Win/Graphwar.exe > windows_release_x86_64.exe` 就是一份 `4.7.stable` 的
  Windows 导出模板。**但本次没有验证**，风险三条：
  (a) 导出器会改写 exe 的图标/版本资源，截断后这份"模板"带的是 Graphwar 的图标；
  (b) Godot 会校验模板与编辑器版本（我们客户端用 **4.7.2**，原版是 **4.7.stable**，不是同一版本号）；
  (c) 没有 `console` 版本的模板，调试输出看不到。
  等级：**推断**。要定论得真跑一次 `godot --export-release "Windows Desktop"`。

---

## 5. 仍未验证的（按 `docs/06` §17 的口径表态）

1. **跨架构是否逐位一致 —— 未验证，本文不承诺。**
   §17.1 那三条（状态 / 消息字节 / 几何）是在**同一个二进制**上定的口径。换架构后：
   * `f64` 的 `sin/cos/exp/ln/pow` 走的是 **libm**（Linux `NEEDED libm.so.6`，
     Android `NEEDED libm.so`，Windows 走 mingw 的 libm）。**不同 libm 的 ULP 差异不会被我们控制。**
   * 所以"采样器在 Android 上逐点等于 Linux"这件事，**本任务没有测，也不能从字节推出**。
     要做只有一条路：在真机/wine 上把 `path_calculate` 的输出导出来，和我们内核的
     `analysis/difftest.py` 那 704 个用例对拍。这条应该单独立项。
2. `user://player.config` 的实际键名/格式（要运行期实物）。
3. `purchase_pending` 的 Rust 侧处理点。
4. `request_permissions_result` 为什么连在 `get_tree()` 上。
5. Windows 那 4 个 broker/房间模块是"死代码未删"还是"源码不同"（§3.6）。
6. Windows 那份的 `[profile.release]` 到底是什么（只能从产物反推）。
7. `Graphwar.exe` 截断是否真能当导出模板（§4.5）。
8. **广告为什么没填充** —— 这是**运行期/网络**问题，静态分析到此为止。缺的三件事：
   ①真机 `adb logcat`（Google 的 `Ads`/`MobileAds` tag）；②设备能否访问
   `googleads.g.doubleclick.net`；③AdMob 后台该广告位的填充率/账号状态。
   **不要把"看不到广告"写成"代码有问题"**——代码层的两条门槛（`owned`、`initialize`）都已经定位到指令。
9. **"原生 AdView 覆盖层 vs Godot 布局"到底互不影响** —— 判 §2.8 ④ 那条判据要用；
   缺一次真机截图 + `dumpsys window` 对照。
10. `GraphwarLogger` 的输出去哪了（`log` 只有 132 B，`new` 里出现 `0.0.0.0:0`、
    `Connection Lost`、`Message too large`，像是自带的 TCP logger）。
    ⇒ **不能假定 `root.log(...)` 会出现在 logcat**；要看广告错误请直接看 AdMob SDK 自己的 logcat tag。

---

## 6. 证据分级附录（每个结论的"凭据"）

| 等级 | 本文用到的地方 | 怎么复核 |
|---|---|---|
| **逐位验证** | 三平台 PCK/资产 md5 全等（§3.1）；`project.binary`/`.gdextension` md5 相同；`purchase_confirmed` 的 2 vs 1 是 strtab（§2.5）；95 条 `src:line` 集合相等（§3.3） | 直接 `md5sum` / 数字节 / 取集合 |
| **观测** | 全部地址与指令（给了 `0x…`）；`are_ads_enabled = !owned`、`get_ad_unit_id = 常量`；两个广告位串；`lock/unlock` 写哪几个槽；`user://player.config`；`GraphwarBackend` 不是 Godot 类；Windows 段表与 COFF 符号表；overflow-check 串只在 Windows | `llvm-objdump -d --start-address=… --stop-address=…`（aarch64）／`objdump -d`（x86-64）／`objdump -h`／`readelf -sW` |
| **推断（写明缺什么）** | "由 cfg 决定广告位 id"（缺 cfg 分支字节）；"Linux 上因缺插件而不工作"（缺一次实机日志）；"皮肤不影响命中"（缺运行期对照）；"Windows 是 LTO/GC 差异"（缺 `Cargo.toml`）；"导出器按 basename 拷库"（缺导出器内部）；"exe 可截断当模板"（缺一次真导出）；"pending 走 ScreenControl::pending"（缺交叉引用） | 缺的东西都写在该条的括号里 |
| **未知** | 跨架构逐位一致性；`player.config` 键名；`request_permissions_result` 语义；`process_button_pressed` 的 `unlock_skins` 臂边界；Windows 4 个模块的归属 | — |

### 6.1 本次**没有**做的事（避免误读）

* 没有反编译 `process_button_pressed`（5.7 KB）与 `ready`（5.6 KB）的完整控制流，只做了**地址级定位**。
* 没有在 Android/Windows 上跑起来（无真机 / 无 NDK / 无 Godot-for-Windows）。
* 没有做任何交叉编译，没有装工具链。
* 没有动 `docs/INDEX.md` / `docs/HANDOVER.md`。
* 没有把任何二进制、APK、`Win/` 内容复制进本仓库（`git ls-files` 自检见提交说明）。
