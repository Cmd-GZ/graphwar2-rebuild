//! **不是原版的东西**：把复原的 [`crate::function_drawer`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版的 `FunctionDrawer` 是一个 `Control` 子类（`graphwarrust::function_drawer::FunctionDrawer`），
//! 它**自己不画任何图元**：`GraphwarPlane::init`（x86 `0x4b7380`）建 **3 个**它并 `add_child`，
//! 它给每个 player id 建一条 `Line2D` 子节点，`update_function` 只把**新增的那几个点**
//! `add_point` 进去（`docs/16-client.md` §36.3：全二进制唯一的 `add_point` 调用点 `0x498fab`）。
//!
//! `src/function_drawer.rs` 已经把「补哪些点」的纯逻辑复原出来了（`append_window` /
//! `CurveStore`），但它按约定**不碰引擎**。本文件是那层薄壳：
//!
//! ```text
//! src/function_drawer.rs                纯逻辑：5 个方法的地址 / 追加窗口 / 点布局常量
//! src/_scaffold/function_drawer_class.rs ← 本文件：真的建 Line2D、真的 add_point、真的 add_child
//! ```
//!
//! 追加判定**只有一份**：本类不写 `if new_len > written`，而是调
//! [`function_drawer::append_window`] —— 那个函数就是 `0x498d5a` 的 `ja` + `&points[written..new_len]`。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 曲线载体 | 每个 player id 一条 `Line2D` 子节点（`add_child_full`，`0x498e93`） | 同：`Line2D::new_alloc()` + `add_child` | **观测** |
//! | 容器 | `HashMap<i32, (Gd<Line2D>, Color, usize)>`，entry **40 B**（`Gd` 16 + `Color` 16 + `usize` 8，entry 48 B） | `BTreeMap<i32, Curve{Gd, Color, written}>`——**只改成有序**，value 字段与顺序照原版 | 布局**观测**（`0x498f13`–`0x498f36` 的插入块）；`BTreeMap` 是**我方选择**（同 `CurveStore`） |
//! | 追加语义 | `new_len <= 已写入数` ⇒ 一个点都不加（`0x498d5a` 的 `ja`）；否则只补 `points[written..new_len]`，每点 `add_point(Vector2, -1)`（`0x498fab`，`esi = 0xffffffff`） | 同：走 [`function_drawer::append_window`] + [`function_drawer::ADD_POINT_INDEX`] | **逐位** |
//! | `new_len` 从哪来 | **独立入参**（`r8`）；`new_len > points.len()` 走切片越界 panic（`function_drawer.rs:45:38`） | 取 `points.len()` ⇒ 那条 panic 分支**按构造不可达**（不变量交给类型，不是把证据删掉） | 原版**逐位**；化简是**我方选择** |
//! | 点的精度 | 点 16 B = 两个 **f64**（`movsd` ×2 + `cvtsd2ss`），`POINT_STRIDE/POINT_X_OFFSET/POINT_Y_OFFSET` | `PackedVector2Array`（Godot 的 `Vector2` 是 **f32**）⇒ 窄化发生在**数组边界**，等价于原版那两条 `cvtsd2ss` | 原版**逐位**；边界窄化**推断**（同一份数只掉一次精度） |
//! | `color` 从哪来 | `r9d`（`0x00RRGGBB`），**只在建节点时用一次**：低字节=R、次字节=G、第三字节=B，各 ÷255，alpha=1（`0x498d74`–`0x498da1`） | 同：`color_from_packed`，仍只在建节点时 `set_default_color` | **观测**（指令级） |
//! | `z_index` | 建节点时 `set_z_index([self+0x40] 的 i32)`（`0x498dee` 取值 / `0x498e49` 传参 / `0x9bb028`）；那个字段就是 2024 符号表里被内联掉的 `set_z_index` 的宿主（§36.1 的**推断**在这一条上更站得住） | **不设**：字段的**初值未知**，本类不发明一个数字（`Line2D` 默认 0） | 原版**观测**；值是**未知** |
//! | `update_transform` 的 4 个被调者 | 每条线：`set_position(Vector2(x,y))`（`0x9bb230`）、`set_width(0.125f32)`（`0x9bbfa0`）、`set_scale(±scale)`（`0x9bb238`）、`set_joint_mode(2)`（`0x9bbfa8`） | 同：顺序、常量、取负都照做 | 常量与取负**逐位**（见 [`LINE_WIDTH`]）；`flip` 标志**观测**；三个入参的**含义未知**（调用方给） |
//! | `update_function_fade` | 按 id 查 entry，把 `alpha as f32` 写进 value 里 `Color` 的**第 4 个 f32**（`0x499377`），写回，再 `set_default_color(那个 Color)`（`0x4993a3`） | 同 | **观测**（指令级） |
//! | `remove_function` | `Node::queue_free` + hashbrown `remove_entry`（`0x498b70`） | 同：`queue_free` + `BTreeMap::remove` | **观测** |
//! | `draw` | arm64 有（560 B：`get_point_count` + `get_point_position` + `draw_circle` 各 1 处在循环里）；x86 **未定位** | **不实现 `_draw`**（画什么本来就不是本类的职责：曲线是节点） | **未知** |
//! | 返回值 | 5 个方法都是 `()` | `update_function` 返回**本次追加了几个点**，其它回 `bool` / 计数 | **我方选择**（验收口；没有它 headless 里分不清"没加"和"加错了"） |
//!
//! ⇒ 这个类证明的是「**纯逻辑的追加窗口能不能真的落到 `Line2D::add_point` 上**」，
//! **不是**"曲线已经画对了"：`GraphwarPlane::draw` 还没喂函数（没有 `GraphwarFunctionManager`），
//! `z_index` 与 `draw` 仍是未知。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --features gdext-bindings
//! cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/function_drawer_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! FunctionDrawer : Control
//!   update_function(player_id: int, points: PackedVector2Array, color: int) -> int
//!                                    # 建/取该 id 的 Line2D，**只**补 points[written..len]；
//!                                    # 变短时一个点都不加；返回本次补了几个点（-1 = id 越界被拒）
//!   remove_function(player_id: int) -> bool                # queue_free 那条线并从表里删
//!   update_transform(x: float, y: float, scale: Vector2, flip: bool) -> int
//!                                    # 对每条线 set_position/set_width(0.125)/set_scale(±scale)
//!                                    # /set_joint_mode(2)；返回动了几条线
//!   update_function_fade(player_id: int, alpha: float) -> float
//!                                    # 覆写 value 里 Color 的 alpha，写回，再 set_default_color；
//!                                    # 返回生效的 alpha（-1.0 = 无此 id）
//!
//!   function_count() -> int                        # 表里有几条曲线（= 几个 Line2D 子节点）
//!   point_count(player_id: int) -> int             # Line2D::get_point_count()（-1 = 无此 id）
//!   written(player_id: int) -> int                 # value 里的"已写入数"（-1 = 无此 id）
//!   stored_alpha(player_id: int) -> float          # value 里 Color 的 alpha（-1.0 = 无此 id）
//!   line_instance_id(player_id: int) -> int        # 那条 Line2D 的 instance id（-1 = 无此 id）
//!
//!   add_point_total() / created_line_total() / removed_line_total() -> int
//!   rejected_call_total() / transform_line_total() / fade_call_total() -> int
//! ```
//!
//! 后面这 6 个计数是**观测点**：`add_point_total` 是**我方**调了几次 `add_point`
//! （不是引擎侧钩子——Godot 的 `Line2D::add_point` 没有信号可连），它只用来对
//! "本次追加了几个点"，真正证明"没重画"的是 `get_point_position(0)` 仍是旧值。
//!
//! # 写这种类必须知道的几条（与 §38.4 / §41.6 同一套，这里只记与本类有关的）
//!
//! 1. **`base_mut()` 是独占借用** ⇒ 建节点那段要先 `create_line()`（内部 `base_mut()`），
//!    等它的借用结束再 `self.lines.get_mut()`。本类比 `EntityDrawer` 好办：建节点只发生在
//!    "表里没有这个 id"时，两条路径不会交叉。
//! 2. **`Gd<Node>` 不保活**：`Line2D` 归本类（父节点）所有，`Gd` 存在表里只是句柄；
//!    `queue_free` 之后**必须**同时从表里删（否则下一次 `add_point` 会碰野句柄 ⇒ `1337` + 卡死）。
//! 3. **树外 `add_child` 只记父子关系**：`--script` 模式的 `_initialize()` 跑在根节点进树之前，
//!    本类不依赖子节点的 `_ready` / `_process`（它们没有这两个虚方法）。
//! 4. **不用写 `set_process(true)`**，本类也没有 `_process`。
//! 5. **`add_point` 的 index = `-1` 要显式传**（`add_point_ex(p).index(ADD_POINT_INDEX)`）：
//!    gdext 的 `add_point()` 恰好也用 `-1` 当默认值，但那**不是**证据；原版是 `esi = 0xffffffff`，
//!    所以常量必须来自 [`function_drawer::ADD_POINT_INDEX`]，不能靠默认值。

use std::collections::BTreeMap;

use godot::classes::line_2d::LineJointMode;
use godot::classes::{Control, IControl, Line2D, Node};
use godot::prelude::*;

use crate::function_drawer::{self, ADD_POINT_INDEX};

/// `Line2D::set_width` 的实参：**0.125**。
///
/// 逐位：`update_transform`（x86 `0x499050`）的两个调用点 `0x49911c` / `0x49921c` 都是
/// `movss xmm0, [rip-…] # 1baff8`，而 `.rodata 0x1baff8` 的 4 个字节是
/// `00 00 00 3e` = `0x3e000000` = `0.125f32`：
///
/// ```text
/// objdump -s --start-address=0x1baff0 --stop-address=0x1bb000 .reference/steam/2026-08-28/libgraphwarrust.so
///  1baff0 63616c6c 4e6f6e65 0000003e 504f5354  callNone...>POST
/// ```
///
/// ⚠️ **纯逻辑里没有这个常量**（`src/function_drawer.rs` 只记了"`set_width`×2"，没记实参），
/// 所以它落在本文件里、带着地址——不是从别处抄的、也不是我方挑的。
pub const LINE_WIDTH: f32 = 0.125;

/// `Line2D::set_joint_mode` 的实参：**2** = `LINE_JOINT_ROUND`。
///
/// 观测：`0x499158` / `0x49925a` 的 `mov esi, 0x2`，紧跟着就是槽 `0x9bbfa8`（`set_joint_mode`）。
pub const LINE_JOINT_MODE: LineJointMode = LineJointMode::ROUND;

/// 一条曲线在表里的 value。原版是 **40 B** 的三元组，顺序就是这三个字段：
///
/// ```text
/// 0x498f17  mov [bucket+0x08], r11      ; Gd<Line2D> 的指针
/// 0x498f1c  mov [bucket+0x10], r10      ; Gd<Line2D> 的 instance id
/// 0x498f21  movdqa xmm0, [rsp+0x30]     ; 建节点时算出来的 Color
/// 0x498f27  movdqu [bucket+0x18], xmm0  ; Color（16 B，alpha 在 +0x24）
/// 0x498f2d  mov [bucket+0x28], 0x0      ; usize 已写入数
/// ```
///
/// ⇒ 原版的 value 是 `(Gd<Line2D>, Color, usize)`，**不是**纯逻辑 §`update_function` 里
/// 写的二元组。多出来的 `Color` 就是 `update_function_fade` 要改的那一份
/// （`0x499391` 读回来、`0x499377` 改 alpha、`0x4993a3` 写回 `Line2D`）。
struct Curve {
    /// 那条 `Line2D` 子节点（原版 value 的 +0x00/+0x08）。
    line: Gd<Line2D>,
    /// 建节点时用的颜色（原版 value 的 +0x10；`update_function_fade` 改它）。
    color: Color,
    /// 已经 `add_point` 进去几个点（原版 value 的 +0x20）。
    written: usize,
}

/// `FunctionDrawer`：每个 player id 一条 `Line2D`，`update_function` **增量追加**点。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct FunctionDrawer {
    base: Base<Control>,

    /// 原版 `HashMap<i32, (Gd<Line2D>, Color, usize)>`（hashbrown）。
    ///
    /// 偏移：`RawTable` 在 `self+0x00`（`[self]` = ctrl、`[self+8]` = bucket_mask、
    /// `[self+0x10]` / `[self+0x18]` = growth_left / items），`self+0x20` 是交给
    /// `0x47dec0`（SipHash）的 hash state，`[self+0x40]` 是 z_index。
    /// ⚠️ `src/function_drawer.rs` 的文档写的是"`self+0x20` 的 `HashMap`"——
    /// 本轮按指令读到的表在 `self+0`（见 `docs/16-client.md` §42.3）。
    ///
    /// 换成 `BTreeMap` 只为**确定性**（与 [`function_drawer::CurveStore`] 同一个理由：
    /// 迭代顺序不参与任何判定）。key 是玩家 id，**没排序**也照样工作。
    lines: BTreeMap<i32, Curve>,

    /// 累计调了几次 `Line2D::add_point`（我方计数，见文件头契约那一段）。
    add_point_total: i64,
    /// 累计建了几条 `Line2D`。
    created_line_total: i64,
    /// 累计 `remove_function` 掉了几条。
    removed_line_total: i64,
    /// 累计被拒的调用（id 超出 i32 / 表里没这个 id）。
    rejected_call_total: i64,
    /// `update_transform` 累计动了几条线。
    transform_line_total: i64,
    /// `update_function_fade` 生效了几次。
    fade_call_total: i64,
}

/// 原版的 `& 0xff` / `>> 8` / `>> 0x10` 三个字节 ÷ 255.0 → `Color(r, g, b, 1)`（`0x498d74`–`0x498da1`）。
///
/// 入参是 GDScript 的 `int`（i64）：**只取低 24 位**，与 `r9d`（低 32 位寄存器）一致。
fn color_from_packed(packed: i64) -> Color {
    let bits = packed as u64;
    let r = (bits & 0xff) as f32 / 255.0;
    let g = ((bits >> 8) & 0xff) as f32 / 255.0;
    let b = ((bits >> 16) & 0xff) as f32 / 255.0;
    Color::from_rgba(r, g, b, 1.0)
}

#[godot_api]
impl IControl for FunctionDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            lines: BTreeMap::new(),
            add_point_total: 0,
            created_line_total: 0,
            removed_line_total: 0,
            rejected_call_total: 0,
            transform_line_total: 0,
            fade_call_total: 0,
        }
    }

    // 原版的 `draw`（arm64 `0x3911b4`）在 x86 侧**没有落点**（`FunctionDrawer::FUNCTIONS`
    // 里 `x86: None`，§36.3 的有界否定）⇒ 本类不覆写 `_draw`：它本来也不画图元。
}

#[godot_api]
impl FunctionDrawer {
    /// 喂一帧的函数点：**只补新增的那一段**。
    ///
    /// 对应 `update_function`（x86 `0x498c70`）：
    ///
    /// 1. 表里没有这个 `player_id` ⇒ 建 `Line2D`（`set_default_color(color)` + `add_child`）；
    /// 2. [`function_drawer::append_window`]（= `0x498d5a` 的 `ja`）：`new_len <= 已写入数` ⇒ **返回，一个点都不加**；
    /// 3. 否则 `points[written..new_len]` 逐点 `Line2D::add_point(p, -1)`（`0x498fab`）；
    /// 4. 追加过才把"已写入数"改成 `new_len`（原版提前 return 那条**不写**）。
    ///
    /// **返回本次追加了几个点**（0 = 变短/相等所以没加，-1 = `player_id` 超出 i32 被拒）。
    ///
    /// `color` 只在**建节点**时用（原版 `r9d` 也只在那里用一次）；之后想改走
    /// [`FunctionDrawer::update_function_fade`]。
    #[func]
    fn update_function(&mut self, player_id: i64, points: PackedVector2Array, color: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            // 原版的 key 是 i32（`0x498caa` 之后按 dword 比较）⇒ 越界不在契约里，拒绝而不是截断。
            self.rejected_call_total += 1;
            godot_warn!("FunctionDrawer::update_function player_id {player_id} 超出 i32");
            return -1;
        };

        // ① 首次见到这个 id 才建节点（原版 `0x498d68` 起的那条分支）。
        if !self.lines.contains_key(&id) {
            self.create_line(id, color);
        }

        // 原版的 `new_len` 是独立入参（`r8`）；本类取数组长度 ⇒ `new_len > points.len()` 不可达。
        let pts = points.as_slice();
        let new_len = pts.len();

        let curve = self.lines.get_mut(&id).expect("刚刚 ensure 过");
        let Some(window) = function_drawer::append_window(curve.written, new_len) else {
            // `new_len <= written`：原版在这里直接 return（曲线**不会**变短）。
            return 0;
        };

        let mut appended = 0i64;
        for i in window {
            let Some(p) = pts.get(i) else {
                // 上界就是 `pts.len()`，这行按构造到不了；真到了就停下而不是 panic。
                break;
            };
            // 原版：`movsd` ×2 + `cvtsd2ss` ⇒ `add_point(Vector2, -1)`（`esi = 0xffffffff`）。
            curve
                .line
                .add_point_ex(*p)
                .index(ADD_POINT_INDEX)
                .done();
            appended += 1;
        }
        curve.written = new_len;
        self.add_point_total += appended;
        appended
    }

    /// 建那条 `Line2D`（原版"首次见到 player_id"分支）：`set_default_color` → `add_child`。
    ///
    /// **没做** `set_z_index`：原版在 `0x498dee` 取 `[self+0x40]`、`0x498e49` 把它交给
    /// `0x9bb028`（`set_z_index`），那个字段的**初值未知** ⇒ 本类不发明一个数字
    /// （`Line2D` 默认 0）。
    fn create_line(&mut self, id: i32, packed_color: i64) {
        let color = color_from_packed(packed_color);

        let mut line = Line2D::new_alloc();
        // `0x498e30`：`set_default_color(Color)`。
        line.set_default_color(color);
        // `0x498e93`：`add_child_full`。子节点归本节点所有（文件头第 2 条）。
        let node: Gd<Node> = line.clone().upcast();
        self.base_mut().add_child(&node);

        self.created_line_total += 1;
        self.lines.insert(
            id,
            Curve {
                line,
                color,
                written: 0,
            },
        );
    }

    /// 删一条曲线：`Node::queue_free` + 从表里删（原版 `0x498b70`）。
    ///
    /// 返回是否真删了。**必须两件事一起做**：只 `queue_free` 不删表 ⇒ 下次 `add_point` 碰野句柄。
    #[func]
    fn remove_function(&mut self, player_id: i64) -> bool {
        let Ok(id) = i32::try_from(player_id) else {
            self.rejected_call_total += 1;
            return false;
        };
        let Some(curve) = self.lines.remove(&id) else {
            self.rejected_call_total += 1;
            return false;
        };
        let mut line = curve.line;
        line.queue_free();
        self.removed_line_total += 1;
        true
    }

    /// 每条线摆位置 + 设宽度 / 缩放 / 接点样式（原版 `update_transform`，x86 `0x499050`）。
    ///
    /// 观测到的循环体（每条线 4 个调用，顺序如下）：
    ///
    /// ```text
    /// set_position(Vector2(x, y))        ; 0x9bb230，取两个 f32 入参
    /// set_width(0.125)                   ; 0x9bbfa0，实参见 LINE_WIDTH
    /// set_scale(±Vector2)                ; 0x9bb238；flip 为真时 xmm2 与 0x19e880 的
    ///                                    ; 四个 -0.0 做 pxor ⇒ 两个分量都取负
    /// set_joint_mode(2)                  ; 0x9bbfa8，实参见 LINE_JOINT_MODE
    /// ```
    ///
    /// ⚠️ **三个入参的含义未知**：`x`/`y` 按被调者 `set_position` 的用法命名，`scale` / `flip`
    /// 是形状（`xmm2` / `esi`）的直接翻译。原版的调用方（`GraphwarPlane::draw`）传什么，
    /// 本切片没接（没有 `GraphwarFunctionManager`）。
    ///
    /// 返回动了几条线（= 表长）。
    #[func]
    fn update_transform(&mut self, x: f64, y: f64, scale: Vector2, flip: bool) -> i64 {
        let s = if flip {
            Vector2::new(-scale.x, -scale.y)
        } else {
            scale
        };
        let position = Vector2::new(x as f32, y as f32);

        let mut touched = 0i64;
        for curve in self.lines.values_mut() {
            curve.line.set_position(position);
            curve.line.set_width(LINE_WIDTH);
            curve.line.set_scale(s);
            curve.line.set_joint_mode(LINE_JOINT_MODE);
            touched += 1;
        }
        self.transform_line_total += touched;
        touched
    }

    /// 改某条曲线的透明度（原版 `update_function_fade`，x86 `0x4992b0`）。
    ///
    /// 原版只做三件事：把 `alpha as f32` 写进 value 里那个 `Color` 的第 4 个 f32
    /// （`0x499377` 的 `movss [……-0xc]`，即 Color 的 alpha 车道），把整份 `Color` 写回 entry，
    /// 再 `set_default_color(那个 Color)`（`0x4993a3`）。
    ///
    /// 返回生效的 alpha（-1.0 = 表里没这个 id）。
    #[func]
    fn update_function_fade(&mut self, player_id: i64, alpha: f64) -> f64 {
        let Ok(id) = i32::try_from(player_id) else {
            self.rejected_call_total += 1;
            return -1.0;
        };
        let Some(curve) = self.lines.get_mut(&id) else {
            self.rejected_call_total += 1;
            return -1.0;
        };
        // 原版不夹取：`cvtsd2ss` 直接窄化（f64 → f32 的饱和语义也照 Rust 的 `as`）。
        curve.color.a = alpha as f32;
        let color = curve.color;
        let mut line = curve.line.clone();
        line.set_default_color(color);
        self.fade_call_total += 1;
        color.a as f64
    }

    /// 表里有几条曲线（= 有几个 `Line2D` 子节点）。
    #[func]
    fn function_count(&self) -> i64 {
        self.lines.len() as i64
    }

    /// 第 `player_id` 条曲线现在有几个点（`Line2D::get_point_count`）。无此 id 返回 -1。
    #[func]
    fn point_count(&self, player_id: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1;
        };
        match self.lines.get(&id) {
            Some(c) => c.line.get_point_count() as i64,
            None => -1,
        }
    }

    /// value 里的"已写入数"。无此 id 返回 -1。
    #[func]
    fn written(&self, player_id: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1;
        };
        match self.lines.get(&id) {
            Some(c) => c.written as i64,
            None => -1,
        }
    }

    /// value 里那个 `Color` 的 alpha。无此 id 返回 -1.0。
    #[func]
    fn stored_alpha(&self, player_id: i64) -> f64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1.0;
        };
        match self.lines.get(&id) {
            Some(c) => c.color.a as f64,
            None => -1.0,
        }
    }

    /// 那条 `Line2D` 的 instance id（GDScript 侧用它从子节点里认人）。无此 id 返回 -1。
    #[func]
    fn line_instance_id(&self, player_id: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1;
        };
        match self.lines.get(&id) {
            Some(c) => c.line.instance_id().to_i64(),
            None => -1,
        }
    }

    /// 累计调了几次 `Line2D::add_point`（**我方**计数）。
    #[func]
    fn add_point_total(&self) -> i64 {
        self.add_point_total
    }

    /// 累计建了几条 `Line2D`。
    #[func]
    fn created_line_total(&self) -> i64 {
        self.created_line_total
    }

    /// 累计删了几条曲线。
    #[func]
    fn removed_line_total(&self) -> i64 {
        self.removed_line_total
    }

    /// 累计被拒的调用（id 越界 / 无此 id）。
    #[func]
    fn rejected_call_total(&self) -> i64 {
        self.rejected_call_total
    }

    /// `update_transform` 累计动了几条线。
    #[func]
    fn transform_line_total(&self) -> i64 {
        self.transform_line_total
    }

    /// `update_function_fade` 生效了几次。
    #[func]
    fn fade_call_total(&self) -> i64 {
        self.fade_call_total
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」，不发明第二份）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/function_drawer_check.gd`（headless，真引擎 + 真 `Line2D`）。
// 这里钉三件容易悄悄跑偏的事：追加窗口、打包颜色 -> Color 的字节序、宽度常量。

#[cfg(test)]
mod tests {
    use super::*;
    use godot::obj::EngineEnum;

    /// 增量追加的判定就是 `append_window`：10 → 12 只补 2 个，12 → 5 一个都不补。
    #[test]
    fn append_window_is_the_only_increment_rule() {
        assert_eq!(function_drawer::append_window(0, 10), Some(0..10));
        assert_eq!(function_drawer::append_window(10, 12), Some(10..12));
        assert_eq!(
            function_drawer::append_window(10, 12).map(|r| r.len()),
            Some(2),
            "10 → 12 只补 2 个点（不是重画 12 个）"
        );
        assert_eq!(function_drawer::append_window(12, 12), None, "相等也不补");
        assert_eq!(function_drawer::append_window(12, 5), None, "变短一个点都不补");
        assert_eq!(function_drawer::append_window(0, 0), None);
    }

    /// `add_point` 的 index 取自纯逻辑（原版 `esi = 0xffffffff`），不是 gdext 的默认值。
    #[test]
    fn add_point_index_comes_from_the_pure_module() {
        assert_eq!(ADD_POINT_INDEX, -1);
        assert_eq!(ADD_POINT_INDEX as u32, 0xffff_ffff, "原版 esi = 0xffffffff");
    }

    /// 打包色 -> `Color`：**低字节是 R**（`0x498d74`–`0x498da1`），alpha 恒 1。
    #[test]
    fn packed_color_low_byte_is_red() {
        let c = color_from_packed(0x11_22_33);
        assert!((c.r - 0x33 as f32 / 255.0).abs() < 1e-6, "r = 低字节 = 0x33");
        assert!((c.g - 0x22 as f32 / 255.0).abs() < 1e-6, "g = 次字节 = 0x22");
        assert!((c.b - 0x11 as f32 / 255.0).abs() < 1e-6, "b = 第三字节 = 0x11");
        assert!((c.a - 1.0).abs() < 1e-6, "alpha 恒 1（原版常量槽 0x19cf70 的尾车道）");
        // 低字节=R ⇒ `0x0000ff` 才是纯红，`0xff0000` 是纯蓝
        // （写成 `0xRRGGBB` 的直觉会反过来；headless 脚本里那条断言是同口径）。
        let red = color_from_packed(0x00_00_ff);
        assert!((red.r - 1.0).abs() < 1e-6, "0x0000ff → 纯红，r={}", red.r);
        assert!(red.g.abs() < 1e-6 && red.b.abs() < 1e-6);
        let blue = color_from_packed(0xff_00_00);
        assert!((blue.b - 1.0).abs() < 1e-6, "0xff0000 → 纯蓝，b={}", blue.b);
        assert!(blue.r.abs() < 1e-6 && blue.g.abs() < 1e-6);
        // 只取低 24 位：高位（含 i64 的符号扩展）不进颜色。
        let high = color_from_packed(0x1234_5678_9abc_deff_u64 as i64);
        assert!((high.r - 1.0).abs() < 1e-6, "r={}", high.r);
        assert!((high.g - 0xde as f32 / 255.0).abs() < 1e-6, "g={}", high.g);
        assert!((high.b - 0xbc as f32 / 255.0).abs() < 1e-6, "b={}", high.b);
    }

    /// 宽度常量是 `.rodata 0x1baff8` 那 4 个字节：`00 00 00 3e` = `0x3e000000` = 0.125。
    #[test]
    fn line_width_is_the_bit_pattern_from_rodata() {
        assert_eq!(LINE_WIDTH.to_bits(), 0x3e00_0000);
        assert!((LINE_WIDTH - 0.125).abs() < f32::EPSILON);
        // 接点样式 = 2（`mov esi, 0x2`）。
        assert_eq!(LINE_JOINT_MODE, LineJointMode::ROUND);
        assert_eq!(LineJointMode::ROUND.ord(), 2);
    }
}
