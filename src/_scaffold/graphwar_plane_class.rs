//! **不是原版的东西**：把复原的 [`crate::graphwar_plane`] 挂成**真 Godot 节点类**的薄壳。
//!
//! # 为什么会有这一份
//!
//! 原版的 `GraphwarPlane` 是**棋盘本体**（`docs/16-client.md` §23）：它自己只画**一个空心黑框**
//! （`draw_set_transform` ×1 + `draw_rect` ×1，`Rect2(-25,-15,50,30)`、黑、宽 `0.2`、
//! **不填充**），曲线**不在**它里面画——它 `init` 里建 **3 个 `FunctionDrawer` 子节点**，
//! 弹道是 `Line2D` 子节点，由那些子节点画（§26.4 的**有界否定**：`draw_line` 在本类里 0 次）。
//!
//! `src/graphwar_plane.rs` 已经把「建几个子节点 / 脏矩形怎么算 / 每帧刷几次 / 画哪个框」
//! 复原成纯逻辑（`GraphwarPlane::init_children` / `ready_plan` / `process_plan` /
//! `obstacle_partial_update_event` / `BOARD_*`），但按约定**不碰引擎**。本文件是那层壳：
//!
//! ```text
//! src/graphwar_plane.rs                 纯逻辑：子节点意图 / 脏矩形 / 棋盘框常量 / 信号名
//! src/_scaffold/graphwar_plane_class.rs ← 本文件：真的 add_child、真的 draw_rect、真的接 resized
//! ```
//!
//! # 本切片**只**做三件本类自己的事（`FunctionDrawer` 已由 §42 的节点类补上）
//!
//! 1. **建 3 个子节点**（`init` 观测）——它们现在是**真的 `FunctionDrawer`**
//!    （`src/_scaffold/function_drawer_class.rs`，`docs/16-client.md` §38）；
//! 2. **脏矩形**：`obstacle_partial_update_event` 的纯数据版 + 每帧冲刷；
//! 3. **画棋盘框**：`draw_set_transform` + `draw_rect`，常量全取 `graphwar_plane::BOARD_*`。
//!
//! 完整棋盘渲染（函数曲线 / 障碍 / 士兵）**不在本文件**：曲线是那 3 个 `FunctionDrawer`
//! 子节点的事，障碍 / 士兵分别归 `ObstacleDrawer` / `EntityDrawer`（都没有节点类）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 子节点 | `init` 里 `Gd::default_instance::<FunctionDrawer>()` ×3 + `Node::add_child` ×3 | 同样 3 个 `FunctionDrawer`（`FunctionDrawer::new_alloc()`）——**不再是 `Control` 占位**（§42） | **观测**（个数 / 类名） |
//! | 子节点的名字 | 没 `set_name` ⇒ 引擎自动名 `@FunctionDrawer@N`（`docs/16` §2.1.10 的 dump 里同族的 `@AxisDrawer@N` / `@EntityDrawer@N` 都是自动名） | 也没 `set_name` ⇒ 自动名 `@FunctionDrawer@N`。**运行时观测**的值是 `@FunctionDrawer@N`（`docs/16` §38.2） | 原版**观测**；本类**不伪造**那个名字 |
//! | 建子节点的时机 | `init` | **`ready`**（`#[func] build_children()` 也能手动调）。理由：gdext 的 `init` 里 `Base` 还没构造完（`InitState::Uninitialized`），`base_mut()` 用不了 | 原版**观测**；时机差是**gdext 约束** |
//! | `draw` 的图元 | `draw_set_transform` ×1（槽 `0x9bc018`）+ `draw_rect` ×1（槽 `0x9bc028`）；`draw_line` **0** 次 | 同 | **逐位**（常量槽）/ **观测**（调用点） |
//! | `draw_set_transform` 的**实参** | 只确证了 GOT 槽，**实参没读到** | 用 `to_pixel` 那套换算当默认值（`PIXEL_OFFSET` / `PX_PER_UNIT`），两个导出属性可改 | 原版**未知**；默认值是**我方选择** |
//! | `ready` 的 3 个订阅 | `add_obstacle_event` / `destroy_obstacle_event` / `clear_obstacles_event`（`READY_EVENTS`） | **没订**（要 `GraphwarBackend`）；只把信号名与事件名**从纯逻辑里取出来用** | 名字与顺序**观测**；订阅**未做** |
//! | `resized` → `on_resized` | 信号名 `"resized"`、处理者名 `"on_resized"`（`.rodata` 两条字面量） | 真连（信号名与处理者名都取 `graphwar_plane::RESIZED_*`，不另写字符串） | **观测** |
//! | `process` 每帧 | `ObstacleDrawer::update` ×2 + 读一次队色（`process_plan` ⇒ `(2, true)`） | 用 `process_plan()` 的 `2` 当"本帧刷几次"并计数；**没有** `ObstacleDrawer` 节点、**没有**队色 | 数字**观测**；刷的是空操作 |
//! | `on_resized` 里那 3 次 `Control::set_*` | 走 GOT 槽，**方法名未解**（`UNKNOWNS` 第 3 条） | 只 `queue_redraw()` | 原版**未知** |
//! | `obstacle_partial_update_event` 的坏 tag | **`panic!()`**（消息是 `.rodata` 的 `"explicit panic"`） | **先拦**：`is_partial_update_tag` 为假就 `godot_warn!` + 返回空数组 | 原版**观测**；本类**故意不同**（panic 穿过 FFI 会 abort 整个进程） |
//! | `clear_state` / `update_entities` | 两个方法存在（`0x4bded0` / `0x4be270`），语义未解 | **不实现**（`UNKNOWNS` 第 5 条：`clear_state` 引用 `EntityDrawer` / `EffectDrawer` 类名的用途未知） | **未知** |
//!
//! ⇒ 这个类证明的是「**子节点意图 / 脏矩形 / 棋盘框常量**能真的落到引擎上」，
//! **不是**"棋盘已经能玩"：3 个子节点是真的 `FunctionDrawer` 了，但**没有任何东西喂函数**
//! （没有 `GraphwarFunctionManager`，`draw` 里那个 `0x456df0` 没接），障碍与士兵也没有节点。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/draw_classes_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarPlane : Control
//!   @export board_scale : float = 20.0    # 棋盘单位 → 像素（PX_PER_UNIT = 1000/50）
//!   @export flip_y      : bool  = true    # 棋盘 y 向上、画布 y 向下 ⇒ 默认取负
//!
//!   build_children() -> int                       # 建 3 个 FunctionDrawer 子节点（幂等），返回个数
//!   function_drawer_count() -> int                # 纯逻辑记下的子节点数（FUNCTION_DRAWER_CHILDREN）
//!   to_pixel(x, y) -> Vector2i                    # graphwar_plane::to_pixel 的搬运
//!   on_obstacle_partial_update(tag, x, y, size) -> PackedInt32Array
//!                                                 # [min_x, min_y, max_x, max_y]；坏 tag 返回空
//!   partial_update_count() -> int
//!   drain_partial_updates() -> int                # 冲刷队列并返回冲掉几个
//!   ready_event_names() -> PackedStringArray      # 原版 ready 会订的 3 个事件（本类没订）
//!   draw_call_count() / drawn_rect_total() -> int
//!   process_call_count() / obstacle_update_total() / resized_call_count() / ready_call_count() -> int
//! ```
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §34.4 实测过，这里只记与本类有关的）
//!
//! 1. **`Gd<Node>` 不保活**：子节点归父节点所有，`add_child` 之后把 `Gd` 丢掉是安全的；
//!    反过来"父节点被回收而手里还留着 `Gd`"就成了野句柄（碰一下就 `1337` + 卡死）。
//!    ⇒ 本类**只**把子节点的 `instance_id()`（`u64`，纯逻辑的 `NodeRef`）记进
//!    [`graphwar_plane::GraphwarPlane::function_drawers`]，一个 `Gd` 都不留。
//! 2. **树外 `add_child` 只记父子关系**：`--script` 模式的 `_initialize()` 跑在根节点进树之前，
//!    那一刻子节点的 `_ready` / `_process` 不会来。本类不依赖它们（`build_children` 自己就完事）。
//! 3. **`resized` 只在 `ready` 里连** ⇒ 树外 `set_size` 不会触发 `on_resized`（连都还没连）。
//! 4. **不用写 `set_process(true)`**：实现了 `_process` 的扩展类由 Godot 4.7 自己开。
//! 5. **`base_mut()` 是独占借用** ⇒ `add_child` 那段里不能同时碰 `self.plane`。本类的做法：
//!    先把父节点取成**局部** `Gd<Node>`（`self.to_gd().upcast()`），再把
//!    `self.plane.init_children(|_| …)` 的闭包只捕获那个局部的父节点。

use godot::classes::{Control, IControl, Node};
use godot::prelude::*;

use super::function_drawer_class::FunctionDrawer;
use crate::graphwar_plane::{self as gp, PixelRect};

/// 默认的棋盘缩放：`PX_PER_UNIT = CANVAS_W / BOARD_W = 1000 / 50 = 20`。
///
/// `PX_PER_UNIT` 本身是**观测**（`to_pixel` 的换算），但把它当 `draw_set_transform` 的
/// `scale` 用是**我方选择**（原版那次变换的实参没读到，见文件头对照表）。
const DEFAULT_BOARD_SCALE: f64 = gp::PX_PER_UNIT;

/// `PixelRect`（**角点**：`min_x/min_y/max_x/max_y`）→ `PackedInt32Array`。
///
/// 不转成 `Rect2i`：原版存的就是角点（`psubd` / `paddd` 那两条），换成
/// `position + size` 的形状就把"原版不是 `Rect2i`"这个事实抹掉了。
fn pixels_of(r: PixelRect) -> PackedInt32Array {
    let mut out = PackedInt32Array::new();
    out.push(r.min_x);
    out.push(r.min_y);
    out.push(r.max_x);
    out.push(r.max_y);
    out
}

/// `GraphwarPlane`：棋盘本体（`Control` 子类）。画棋盘框 + 持 3 个 `FunctionDrawer` + 收脏矩形。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct GraphwarPlane {
    base: Base<Control>,

    /// 棋盘单位 → 像素的缩放（默认 [`DEFAULT_BOARD_SCALE`]）。
    #[export]
    board_scale: f64,

    /// 是否把 y 轴翻过来（棋盘 y 向上、Godot 画布 y 向下）。
    #[export]
    flip_y: bool,

    /// 纯逻辑的**意图结构**：3 个 `FunctionDrawer` 的 `NodeRef` + 脏矩形队列 + ready 标志。
    ///
    /// 原版对应 3 个 `Gd<FunctionDrawer>` 字段 + 一个 `Vec<PixelRect>`（`+0x50/+0x58/+0x60`）；
    /// 本类只存 id（文件头第 1 条）。
    plane: gp::GraphwarPlane,

    /// `_draw` 被引擎调用过几次。
    draw_calls: i64,
    /// 累计发出过几条 `draw_rect`（恒 `draw_calls`，除非 `_draw` 中途返回）。
    rect_total: i64,
    /// `_process` 被引擎调用过几次。
    process_calls: i64,
    /// 累计"刷了 ObstacleDrawer 几次"（`process_plan()` 说每帧 2 次）。
    obstacle_update_total: i64,
    /// `ready` 被引擎调用过几次。
    ready_calls: i64,
    /// `on_resized` 被调到过几次。
    resized_calls: i64,
    /// 累计从脏矩形队列里**冲掉**了几个框。
    flushed_total: i64,
    /// 因为脏矩形而请求重画的次数。
    dirty_redraws: i64,
}

#[godot_api]
impl IControl for GraphwarPlane {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            board_scale: DEFAULT_BOARD_SCALE,
            flip_y: true,
            // 纯逻辑的 `new()` = `Default::default()`：3 个 id 表 + 空脏队列 + ready=false。
            plane: gp::GraphwarPlane::new(),
            draw_calls: 0,
            rect_total: 0,
            process_calls: 0,
            obstacle_update_total: 0,
            ready_calls: 0,
            resized_calls: 0,
            flushed_total: 0,
            dirty_redraws: 0,
        }
    }

    /// `_ready`：原版在这里连 `resized` + 订 3 个事件。
    ///
    /// 本类：① 建 3 个 `FunctionDrawer` 子节点（原版在 `init`，见文件头对照表）；② 用纯逻辑给的
    /// **信号名 / 处理者名**连 `resized`；③ 3 个事件**没订**（事件名照样能从
    /// [`GraphwarPlane::ready_event_names`] 读到，只是没真的订阅）。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // ① 子节点（原版 `init` 观测：3 个 FunctionDrawer）。
        self.build_children();

        // ② `ready_plan()` 同时把纯逻辑的 `ready` 标志置 true，并给出 (信号名, 处理者名)。
        let ((signal, handler), _events) = self.plane.ready_plan();
        let myself = self.to_gd();
        let callable = myself.callable(handler);
        let err = self.base_mut().connect(signal, &callable);
        if err != godot::global::Error::OK {
            godot_warn!("GraphwarPlane::ready 连接 {signal} 失败: {err:?}");
        }
    }

    /// `_process`：原版每帧把 `ObstacleDrawer::update` 刷 **2** 次并读一次队色
    /// （`process_plan() -> (2, true)`）。
    ///
    /// 本类：照 `2` 计数（没有 `ObstacleDrawer` 节点可刷），
    /// 再把脏矩形队列冲掉——冲掉过东西才 `queue_redraw()`。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;

        let (updates, _reads_team) = self.plane.process_plan();
        self.obstacle_update_total += updates as i64;

        let flushed = self.plane.partial_updates.len() as i64;
        if flushed > 0 {
            self.plane.partial_updates.clear();
            self.flushed_total += flushed;
            self.dirty_redraws += 1;
            self.base_mut().queue_redraw();
        }
    }

    /// `_draw`：原版 `GraphwarPlane::draw`（vaddr `0x4b5df0`）。
    ///
    /// 只有两个图元：`draw_set_transform` ×1（`0x4b607d`，槽 `0x9bc018`）+
    /// `draw_rect` ×1（`0x4b61ab`，槽 `0x9bc028`）。`draw_line` **0 次**（有界否定）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        let scale = self.board_scale as f32;
        let scale_y = if self.flip_y { -scale } else { scale };

        {
            let mut base = self.base_mut();
            // 实参**未知**（原版只确证了槽）；默认值来自 `to_pixel` 的换算（`PIXEL_OFFSET`）。
            base.draw_set_transform_ex(Vector2::new(
                gp::PIXEL_OFFSET.0 as f32,
                gp::PIXEL_OFFSET.1 as f32,
            ))
            .rotation(0.0)
            .scale(Vector2::new(scale, scale_y))
            .done();

            // 四个常量全部来自 `graphwar_plane::BOARD_*`（逐位：`.rodata 0x19cc20` / `0x19cf70`
            // / `0x1ba598`，两个 `xor` 给的 `false`）。
            let rect = Rect2::new(
                Vector2::new(gp::BOARD_RECT.0, gp::BOARD_RECT.1),
                Vector2::new(gp::BOARD_RECT.2, gp::BOARD_RECT.3),
            );
            let color = Color::from_rgba(
                gp::BOARD_COLOR.0,
                gp::BOARD_COLOR.1,
                gp::BOARD_COLOR.2,
                gp::BOARD_COLOR.3,
            );
            base.draw_rect_ex(rect, color)
                .filled(gp::BOARD_FILLED)
                .width(gp::BOARD_WIDTH)
                .antialiased(gp::BOARD_ANTIALIASED)
                .done();
        }
        self.rect_total += 1;
    }
}

#[godot_api]
impl GraphwarPlane {
    /// 建 3 个 `FunctionDrawer` 子节点（`FUNCTION_DRAWER_CHILDREN`）。幂等：已经有就不重复建。
    ///
    /// 名字**不自拟**：原版没 `set_name` ⇒ 引擎自动名（运行时是 `@FunctionDrawer@N`）。
    ///
    /// 返回子节点个数（恒 `3`）。
    #[func]
    fn build_children(&mut self) -> i64 {
        if !self.plane.function_drawers.is_empty() {
            return self.plane.function_drawers.len() as i64;
        }

        // 父节点取成**局部** `Gd<Node>`：闭包里就不必再借 `self`（`base_mut()` 是独占借用）。
        let mut parent: Gd<Node> = self.to_gd().upcast::<Node>();

        // `init_children` 把闭包返回的 id 依次记进 `plane.function_drawers`（纯逻辑的意图结构）。
        self.plane.init_children(|_class_name| {
            // 原版这里是 `Gd::default_instance::<FunctionDrawer>()`；现在是**真的**
            // `FunctionDrawer` 节点类（`docs/16-client.md` §38）。
            let child = FunctionDrawer::new_alloc();
            let node = child.clone().upcast::<Node>();
            parent.add_child(&node);
            // 原版没 `set_name` ⇒ 自动名（`@FunctionDrawer@N`）。
            // 句柄到这里就可以丢掉：节点归**父节点**所有（文件头第 1 条）。
            // `InstanceId::to_i64()` 就是原版 `NodeRef` 要的那个 u64（纯逻辑只拿它当不透明 id）。
            child.instance_id().to_i64() as u64
        });
        debug_assert_eq!(self.plane.function_drawers.len(), gp::FUNCTION_DRAWER_CHILDREN);
        self.plane.function_drawers.len() as i64
    }

    /// 纯逻辑记下的子节点数（`FUNCTION_DRAWER_CHILDREN` = 3）；还没建时是 0。
    #[func]
    fn function_drawer_count(&self) -> i64 {
        self.plane.function_drawers.len() as i64
    }

    /// 棋盘坐标 → 像素（`graphwar_plane::to_pixel`：`x*1000/50 + 500`、`y*1000/50 + 300`）。
    #[func]
    fn to_pixel(&self, x: f64, y: f64) -> Vector2i {
        let (px, py) = gp::to_pixel(x, y);
        Vector2i::new(px, py)
    }

    /// 收一个脏矩形：`obstacle_partial_update_event` 的纯数据版。
    ///
    /// 返回 `PackedInt32Array[min_x, min_y, max_x, max_y]`（**角点**，不是 `Rect2i`）。
    ///
    /// ⚠️ 坏 tag 原版是 **`panic!()`**；本类**先拦**并返回空数组（panic 穿过 FFI 会 abort
    /// 整个进程）——这是一处**故意**与原版不同的行为，见文件头对照表。
    #[func]
    fn on_obstacle_partial_update(
        &mut self,
        tag: i64,
        x: f64,
        y: f64,
        size: f64,
    ) -> PackedInt32Array {
        let tag = tag.clamp(0, u32::MAX as i64) as u32;
        if !gp::is_partial_update_tag(tag) {
            godot_warn!(
                "GraphwarPlane::on_obstacle_partial_update 坏 tag {tag}（原版在这里 panic!）"
            );
            return PackedInt32Array::new();
        }
        pixels_of(self.plane.on_obstacle_partial_update(tag, x, y, size))
    }

    /// 脏矩形队列现在有几个。
    #[func]
    fn partial_update_count(&self) -> i64 {
        self.plane.partial_updates.len() as i64
    }

    /// 冲刷脏矩形队列，返回冲掉几个（`_process` 每帧也会自动冲一次）。
    #[func]
    fn drain_partial_updates(&mut self) -> i64 {
        let n = self.plane.partial_updates.len() as i64;
        self.plane.partial_updates.clear();
        self.flushed_total += n;
        n
    }

    /// 原版 `ready` 会按顺序订阅的 3 个事件名（`READY_EVENTS`）——**本类没订**，只读名字。
    #[func]
    fn ready_event_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for (name, _slot) in gp::READY_EVENTS {
            out.push(&GString::from(name));
        }
        out
    }

    /// `resized` 信号的处理者（原版名字就是 `on_resized`，`.rodata 0x1c2a7a`）。
    ///
    /// 它是 `#[func]` ⇒ GDScript 也能直接调（验收脚本用它验"树外不会自动触发"）。
    #[func]
    fn on_resized(&mut self) {
        self.resized_calls += 1;
        // 原版这里做 3 次 `Control::set_*`，方法名未解（`UNKNOWNS` 第 3 条）⇒ 只重画。
        self.base_mut().queue_redraw();
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// 累计发出过几条 `draw_rect`。
    #[func]
    fn drawn_rect_total(&self) -> i64 {
        self.rect_total
    }

    /// `_process` 被引擎调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// 累计"刷了障碍绘制器几次"（每帧 `OBSTACLE_DRAWER_UPDATES_PER_FRAME` = 2 次）。
    #[func]
    fn obstacle_update_total(&self) -> i64 {
        self.obstacle_update_total
    }

    /// `_ready` 被引擎调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `on_resized` 被调到过几次。
    #[func]
    fn resized_call_count(&self) -> i64 {
        self.resized_calls
    }

    /// 累计从脏矩形队列里冲掉过几个框。
    #[func]
    fn flushed_total(&self) -> i64 {
        self.flushed_total
    }

    /// 因为脏矩形而请求重画的次数。
    #[func]
    fn dirty_redraw_count(&self) -> i64 {
        self.dirty_redraws
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 个子节点这个数字来自纯逻辑，不是本类自己写的 3。
    #[test]
    fn child_count_comes_from_the_pure_module() {
        assert_eq!(gp::FUNCTION_DRAWER_CHILDREN, 3);
        let mut plane = gp::GraphwarPlane::new();
        let mut n = 0;
        plane.init_children(|_name| {
            n += 1;
            n as u64
        });
        assert_eq!(plane.function_drawers.len(), gp::FUNCTION_DRAWER_CHILDREN);
        assert_eq!(plane.function_drawers, vec![1, 2, 3]);
    }

    /// 默认缩放取的是 `PX_PER_UNIT`（1000/50 = 20），棋盘框是 50×30 个单位 ⇒ 1000×600 像素。
    #[test]
    fn default_scale_maps_the_board_onto_the_canvas() {
        assert!((DEFAULT_BOARD_SCALE - 20.0).abs() < 1e-9);
        assert!((gp::BOARD_RECT.2 * DEFAULT_BOARD_SCALE as f32 - gp::CANVAS_W as f32).abs() < 1e-3);
        assert!((gp::BOARD_RECT.3 * DEFAULT_BOARD_SCALE as f32 - gp::CANVAS_H as f32).abs() < 1e-3);
        // 棋盘框的角在像素空间里正好是 (0,0) 与 (1000,600)。
        let (x0, y0) = gp::to_pixel(gp::BOARD_RECT.0 as f64, gp::BOARD_RECT.1 as f64);
        assert_eq!((x0, y0), (0, 0), "左下角 → 画布原点");
        let (x1, y1) = gp::to_pixel(
            (gp::BOARD_RECT.0 + gp::BOARD_RECT.2) as f64,
            (gp::BOARD_RECT.1 + gp::BOARD_RECT.3) as f64,
        );
        assert_eq!((x1, y1), (1000, 600), "右上角 → 画布右下角");
    }

    /// 脏矩形的换算与手算一致，且**只接受两个 tag**。
    #[test]
    fn partial_update_rect_matches_the_hand_computed_pixels() {
        // (5, 5) → (600, 400)；半径 = size*1000/50 = 1.5*20 = 30。
        let mut plane = gp::GraphwarPlane::new();
        let r = plane.on_obstacle_partial_update(gp::ENTITY_TAG_A, 5.0, 5.0, 1.5);
        assert_eq!(
            r,
            PixelRect {
                min_x: 570,
                min_y: 370,
                max_x: 630,
                max_y: 430
            }
        );
        assert_eq!((r.width(), r.height()), (60, 60), "正方形，不是圆");
        assert_eq!(plane.partial_updates.len(), 1);
        // 第二个合法 tag 也收。
        plane.on_obstacle_partial_update(gp::ENTITY_TAG_B, -5.0, -5.0, 0.5);
        assert_eq!(plane.partial_updates.len(), 2);
        assert!(!gp::is_partial_update_tag(0));
        assert!(!gp::is_partial_update_tag(0x27));
    }

    /// `ready_plan()` 给出的信号名 / 处理者名 / 3 个事件名就是本类用的那几个字符串。
    #[test]
    fn ready_plan_supplies_the_names_the_node_class_uses() {
        let mut plane = gp::GraphwarPlane::new();
        let ((signal, handler), events) = plane.ready_plan();
        assert_eq!(signal, "resized");
        assert_eq!(handler, "on_resized");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], "add_obstacle_event");
        assert_eq!(events[1], "destroy_obstacle_event");
        assert_eq!(events[2], "clear_obstacles_event");
        assert!(plane.ready, "ready_plan() 会把标志置 true");
    }

    /// `process_plan()` 说每帧刷 2 次、读一次队色；本类只用了那个 2。
    #[test]
    fn process_plan_reports_two_obstacle_updates_per_frame() {
        let plane = gp::GraphwarPlane::new();
        assert_eq!(
            plane.process_plan(),
            (gp::OBSTACLE_DRAWER_UPDATES_PER_FRAME, true)
        );
        assert_eq!(gp::OBSTACLE_DRAWER_UPDATES_PER_FRAME, 2);
    }
}
