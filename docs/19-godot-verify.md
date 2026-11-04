# 19 · 在真实 Godot 里验证（本轮新增）

## 1. 拿到了引擎

```
Godot_v4.7.2-stable_linux.x86_64 --version
→ 4.7.2.stable.official.ed1daf0bf
```

与 `Graphwar.pck` 记录的 `godot 4.7.0` 同 minor。74.3 MB，放在 `tools/`（gitignore）。

本机 `~` 只读，所以要给引擎三个目录：

```bash
XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache \
XDG_CONFIG_HOME=$PWD/.godot-config tools/Godot_v4.7.2-stable_linux.x86_64 --headless ...
```

## 2. 我们的扩展被真引擎加载（观测）

`godot-probe/`：一个最小项目 + `gw2.gdextension` 指向我们的 `.so`。

```
[ 33% ] 正在校验 GDExtension……
[gw2-rebuild] initialize level=0   ← CORE
[gw2-rebuild] initialize level=1   ← SERVERS
[gw2-rebuild] initialize level=2   ← SCENE
[gw2-rebuild] initialize level=3   ← EDITOR
[gw2-rebuild] resolve: class4=true string_name=true construct=true set_instance=true
[gw2-rebuild] registered class GW2Rebuild (parent RefCounted)
```

四个初始化级别全部触发，接口全部解析成功。

## 3. 类注册被 GDScript 确认（观测）

```gdscript
print("GW2_HAS_CLASS=", ClassDB.class_exists("GW2Rebuild"))
var o = ClassDB.instantiate("GW2Rebuild")
print("GW2_INSTANCE=ok class=", o.get_class())
```

```
GW2_HAS_CLASS=true
GW2_INSTANCE=ok class=GW2Rebuild
```

用的是 `classdb_register_extension_class4` + `GDExtensionClassCreationInfo4`
（21 个函数指针 + `class_userdata`，字段序照 `godot-cpp/4.4` 头文件）。
实例化走 `classdb_construct_object("RefCounted")` + `object_set_instance`。

**这比 `dlopen` 探针强一档**：不是"按 ABI 约定能调通"，而是**引擎真的认了这个类并造出实例**。

## 3.5 方法注册与调用（本轮新增）

注册方法需要 `GDExtensionClassMethodInfo`。**第一次尝试让引擎 abort 了**，
原因是我漏了结构体末尾两个字段：

```c
uint32_t default_argument_count;
GDExtensionVariantPtr *default_arguments;
```

补上后正常。实测：

```
[gw2-rebuild] registered method ping()
GW2_HAS_CLASS=true
GW2_INSTANCE=ok
GW2_HAS_PING=true
[gw2-rebuild] ping() called from GDScript   ← Rust 侧回调真的执行了
GW2_PING_DONE
```

`ping()` 无参数、无返回值，所以不需要构造 Variant；它证明的是
**"引擎 → 我们代码"的调用链通了**。下一步才是带参数/返回值的方法。

> 教训：跨语言结构体必须逐字段核对头文件，少一个尾部字段就可能越界读栈。

## 4. 把原始扩展装进同一个引擎（关键尝试）

`godot-ref/`：把 `libgraphwarrust.so` 当 GDExtension 加载。

- **编辑器模式会核心转储**（扩展需要游戏运行态），记录如下；
- **运行模式可用**：

```
Initialize godot-rust (API v4.6.stable.official, runtime v4.7.2.stable.official, safeguards balanced)
```

于是可以问引擎："这个扩展注册了什么？"

## 5. 原扩展的完整 API 面（观测）

与"不带该扩展的基线"对比（1066 vs 1055 个类），差异 **12 个类**：

```
AxisDrawer  EffectDrawer  EntityDrawer  GraphwarAdSwitch  GraphwarButton
GraphwarFunctionInput  GraphwarLanVBox  GraphwarLongPressAnimation
GraphwarPlane  GraphwarRootNode  GraphwarSetupControl  PlayerShowcase
```

方法一共只有 **25 个**，且几乎全是 UI 回调：

| 类 | 方法 |
|---|---|
| `GraphwarFunctionInput` | `on_text_change`、`on_text_submitted`、`on_size_changed`、`force_update_cb` |
| `GraphwarRootNode` | `request_permissions_result`、`purchase_confirmed/pending/denied`、`log` |
| `GraphwarSetupControl` | `on_text_change`、`on_visibility_change`、`force_title_update`、`on_chat_submitted` |
| `GraphwarPlane` / `EntityDrawer` | `on_resized` |
| `GraphwarButton` | `pressed`、`button_up/down`、`get/set_input_text`… |
| `GraphwarAdSwitch` | `are_ads_enabled`、`get_ad_unit_id` |
| 其余 4 个类 | **没有任何方法** |

### 5.1 这条结论很重要（而且是负面的）

**玩法逻辑没有开放给 GDScript。** 回合、关卡生成、AI、命中结算都不在这 12 个类的方法里——
它们只在 Rust 内部运行，通过场景节点和信号驱动。

因此：**拿不到"调一个方法就能问出回合逻辑"的 oracle。**
之前设想的"用 GDScript 驱动原版逻辑"这条路，在 API 层面被否定。
剩下的只能是：(a) 反编译 + 交叉验证；(b) 运行整个游戏做端到端观察（尚无可行手段）。

## 6. 复现

```bash
# 我们的扩展
cd godot-probe && ../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --editor --path . --quit
# 原扩展的 API 面
cd godot-ref   && ../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path . 
```
