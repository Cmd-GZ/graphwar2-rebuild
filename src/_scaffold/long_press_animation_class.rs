//! **不是原版的东西**：把复原的 [`crate::graphwar_long_press_animation`] 挂成**真 Godot 节点类**的薄壳。
//!
//! # 为什么会有这一份
//!
//! 原版的 `GraphwarLongPressAnimation` 是一个 `Control` 子类，**唯一的虚函数是 `draw`**
//! （2.0.60 arm64 `.symtab` 里这个模块只剩 `draw`，见纯逻辑模块的 §2）：
//!
//! ```text
//! 读自己的启动时刻(+0x18 secs / +0x20 nanos)
//!   → nanos == 1e9 ⇒ 直接返回（没在动画）
//!   → elapsed = Instant::elapsed()
//!   → angle = elapsed × 4π
//!   → draw_arc 两次（黑底 + 白层）
//!   → angle > 2π ⇒ 把 nanos 写回 1e9（结束）
//! ```
//!
//! `src/graphwar_long_press_animation.rs` 已经把上面每一段算术复原成纯函数
//! （`elapsed_secs` / `spin_angle` / `is_finished` / `geometry` / `draw_plan` / `draw_frame`），
//! 但按约定**不碰引擎**。本文件就是"让那两次绘制真的落到 `CanvasItem::draw_arc` 上"的那层壳：
//!
//! ```text
//! src/graphwar_long_press_animation.rs          纯逻辑：状态 / 角度 / 几何 / 两次绘制的实参
//! src/_scaffold/long_press_animation_class.rs   ← 本文件：建类 + 把实参搬进 draw_arc + 观测计数
//! ```
//!
//! # 一处**就地更正**：被调的 API 是 `draw_arc`，不是"矩形"
//!
//! 纯逻辑模块的 §0 写的是"画**两层矩形**（黑底 + 白层）"，§6 把被调 API 记为**未知**。
//! 本文件按 `docs/16-client.md` 的 GOT 槽表（§26 之后那张「绘制 API → 包装 → 槽」表，
//! 行 `draw_arc | 0x19f9d0 | 0x8abca0 | 0x9bc020 | 5 | EntityDrawer::draw ×3 +
//! GraphwarLongPressAnimation::draw ×2`）把它落成 **`CanvasItem::draw_arc`**：
//!
//! | 证据 | 内容 | 档位 |
//! |---|---|---|
//! | GOT 槽 | `GraphwarLongPressAnimation::draw` 里两处 `call *[0x9bc020]`（`0x4d8485` / `0x4d84ed`）就是全 `.text` 里引用该槽的 5 条中的 2 条 | **观测** |
//! | 槽 → 包装 → API | `0x9bc020` ← `0x8abca0` ← 字面量 `0x19f9d0` = `draw_arc`（`R_X86_64_RELATIVE` 写入） | **观测** |
//! | 形参个数与类型 | 6 个 f32（xmm0..xmm5）+ 1 个 int（`esi=0x1e`）+ 1 个 `Color*`（`rdx`）+ 1 个 bool（`ecx=0`）—— 与 `draw_arc(center, radius, start_angle, end_angle, point_count, color, width, antialiased)` 的形参表**逐槽对齐** | **观测**（个数/类型）/ **推断**（哪一槽是哪个形参） |
//!
//! 这也解释了 §0 那条"两层矩形"为什么站不住：`draw_rect` 的实参是 4 个 f32 + 颜色 + 2 个
//! 标量，装不进 6 个 f32 槽；而**旋转角从 0 走到 2π**这件事本身也只有在"弧"上才有意义。
//!
//! # 与原版的差别（逐条）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 两次绘制 | `draw_arc` ×2：黑 `(-0.2 → angle+0.2)` 宽 `2·0.2·v4c+v4c`；白 `(0 → angle)` 宽 `v4c` | 同（实参整个来自 [`graphwar_long_press_animation::draw_plan`]） | **观测**（算术）+ **推断**（形参对应） |
//! | 点数 / 抗锯齿 | `esi = 0x1e`（30）、`ecx = 0`（false） | 直接用 `DrawCall.esi` / `DrawCall.ecx`，不另写常量 | **观测** |
//! | 颜色 | 黑 `{0,0,0,1}`（`.rodata 0x19cf70`）、白 `{1,1,1,1}`（`0x19ed00`） | 同（`Color::BLACK` / `Color::WHITE`） | **观测**；白与 `PlayerShowcase::draw` **共用同一个 `0x19ed00`** |
//! | 三次取值 | 3 次 GOT 槽调用（`p0,p1` / `s0,s1` / `n,c`），**API 名与语义未知** | 位置取 `Control::get_position()`、尺寸取 `Control::get_size()`、`n`/`c` 做成导出属性 | 原版**未知**；本类的取法是**推断** |
//! | 谁来推进动画 | **没人**：2.0.60 里本类只有 `draw`，重画由别的节点触发 | **本类加了 `_process`**（每帧累加 elapsed + `queue_redraw`），否则 headless 里它永远不动 | **我方选择** |
//! | 启动 / 停止 | `GraphwarButton::button_down` 写 `Instant::now()`、`button_up` 写哨兵 `1e9`（**别人**写 `+0x18/+0x20`） | 公开 [`GraphwarLongPressAnimation::press`] / [`GraphwarLongPressAnimation::release`]，由调用方驱动 | 写入者**观测**；本类的接口是**我方选择** |
//! | 长按阈值 | `GraphwarButton::init` 的 `Duration{0, 0x1dcd6500}` = 500 ms，与本类动画时长 0.5 s **数值相同** | 导出属性 `long_press_ms = 500`，**只做展示，不参与绘制** | **观测**（数值）/ **推断**（同一个常量） |
//!
//! ⇒ 这个类证明的是「**纯逻辑算出的两次 `draw_arc` 实参真的能被引擎接受**」，
//! **不是**"这个动画的用途已复原"（"长按进度环"这句话本身在纯逻辑模块里就标着**推断**）。
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
//! GraphwarLongPressAnimation : Control
//!   @export long_press_ms : int = 500     # 长按阈值（+0x58），只做展示
//!   @export geometry_n   : int = 0        # 第三次取值的 i32（语义未知）
//!   @export geometry_c   : float = 0.0    # 第三次取值的 f32（语义未知）
//!
//!   press() -> void                       # 等价 GraphwarButton::button_down：启动时刻 = 现在
//!   advance(delta: float) -> void         # 本类新增：手动推进 delta 秒（与 _process 同一段算术）
//!   release() -> void                     # 等价 button_up：写哨兵（动画立即停）
//!   is_running() -> bool                  # SpinState::is_running（nanos != 1e9）
//!   elapsed() -> float                    # 本类自己累加的秒数
//!   angle() -> float                      # elapsed × 4π（最近一次算出来的）
//!   spin_duration_secs() -> float         # 2π / 4π = 0.5（SPIN_DURATION_SECS）
//!   draw_call_count() / process_call_count() / arc_total() / finished_count() -> int
//! ```
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §38.4 实测过，这里只记与本类有关的）
//!
//! 1. **`base_mut()` 是独占借用**（`BaseMut<'_, Self>` 握着 `&mut self`）⇒ 画东西那段里
//!    不能写 `self.arc_total += 2`（`E0499`）。本类把计数放在 `base` 作用域**之后**。
//! 2. **不用写 `set_process(true)`**：实现了 `_process` 的扩展类由 Godot 4.7 自己开。
//! 3. **`draw_*` 只能在 `NOTIFICATION_DRAW` 里调**（引擎侧 `ERR_DRAW_GUARD`）⇒ 推进状态只
//!    `queue_redraw()`，真正的 `draw_arc` 只在 `_draw` 里发。
//! 4. **"最后一次绘制"必须发生**：原版是**先画两弧、再判断是否该写哨兵**（`0x4d8485` /
//!    `0x4d84ed` 在前，`0x4d8503` 在后）⇒ 本类的 `_draw`（不是 `_process`）负责写哨兵，
//!    否则超时那一帧会被吞掉。

use godot::classes::{Control, IControl};
use godot::prelude::*;

use crate::graphwar_long_press_animation::{
    self as lpa, Color as FxColor, DrawCall, Frame, Geometry, SpinState,
};

/// 把纯逻辑的 `Color`（RGBA 四个 f32）搬成 Godot 的 `Color`。
fn color_of(c: FxColor) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a)
}

/// 一次 `draw_arc` 的形参，**逐个**对应 `DrawCall` 的槽位。
///
/// 形参顺序照 `CanvasItem::draw_arc(center, radius, start_angle, end_angle, point_count,
/// color, width, antialiased)`；槽位对应关系是**推断**（见文件头那张表）：
///
/// ```text
/// xmm0, xmm1 = center（x, y）      xmm2 = radius
/// xmm3       = start_angle         xmm4 = end_angle
/// esi        = point_count         rdx  = color（颜色指针）
/// xmm5       = width               ecx  = antialiased
/// ```
fn arc_args(call: &DrawCall) -> ((f32, f32), f32, f32, f32, i32, FxColor, f32, bool) {
    (
        (call.xmm0, call.xmm1),
        call.xmm2,
        call.xmm3,
        call.xmm4,
        call.esi as i32,
        call.color,
        call.xmm5,
        call.ecx != 0,
    )
}

/// 把累加的秒数拆成 `(secs, nanos)`：原版的 `Instant` 是 `(u64 secs, u32 nanos)` 两个字段
/// （`+0x18` / `+0x20`），纯逻辑的 `elapsed_secs` 也按这个形状吃参数。
///
/// 用 `floor` 而不是 `as`：负数 `as u64` 会饱和成 0，而 `floor` 之后再夹一次更直白。
fn split_secs(elapsed: f64) -> (u64, u32) {
    if !(elapsed > 0.0) {
        return (0, 0);
    }
    let secs = elapsed.floor();
    let frac = elapsed - secs;
    let nanos = (frac * f64::from(lpa::NANOS_PER_SEC)).round();
    let nanos = nanos.clamp(0.0, f64::from(lpa::NANOS_PER_SEC - 1)) as u32;
    (secs as u64, nanos)
}

/// `GraphwarLongPressAnimation`：转一圈就停的 `draw_arc` 动画（`Control` 子类）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct GraphwarLongPressAnimation {
    base: Base<Control>,

    /// 长按阈值（毫秒）。原版是 `GraphwarButton::init` 里的 `Duration{0, 0x1dcd6500}` = 500 ms，
    /// 与 `SPIN_LIMIT / SPIN_RATE = 0.5 s` **数值相同**（推断：同一个常量）。
    ///
    /// ⚠️ 本类**不用**它做任何判定——绘制的结束判据只有 `angle > 2π`（`0x4d8503`）。
    #[export]
    long_press_ms: i64,

    /// 第三次取值拿到的 i32（纯逻辑里叫 `n`）。语义**未知**，只喂给
    /// [`lpa::geometry`] 的 `branch_positive` 判据。默认 `0` ⇒ `(0/2 as f32) > 0.0` 为假。
    #[export]
    geometry_n: i64,

    /// 第三次取值拿到的 f32（纯逻辑里叫 `c`）。语义**未知**，同上。
    #[export]
    geometry_c: f64,

    /// 动画状态：`+0x18`（secs）/ `+0x20`（nanos）。
    ///
    /// 原版由 `GraphwarButton::button_down` 写 `Instant::now()`、`button_up` 写哨兵 `1e9`；
    /// 本类用 `started(0, 0)`（把"零点"当启动时刻）配自己累加的 `elapsed`，等价地走同一条算术。
    spin: SpinState,

    /// 自己累加的 elapsed 秒（替代 `Instant::elapsed()`：headless 里 `delta` 是引擎给的，
    /// 用真实时钟会让验收脚本的期望值没法手算）。
    elapsed: f64,

    /// 最近一次算出来的角度（`elapsed × 4π`）。观测点。
    last_angle: f64,

    /// `_draw` 被引擎调用过几次。
    draw_calls: i64,
    /// `_process` 被引擎调用过几次。
    process_calls: i64,
    /// 累计发出过几条 `draw_arc`（每次绘制恒 2 条）。
    arc_total: i64,
    /// 动画**自己**结束过几次（`angle > 2π` 那条分支）。
    finished_count: i64,
}

#[godot_api]
impl IControl for GraphwarLongPressAnimation {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            long_press_ms: 500,
            geometry_n: 0,
            geometry_c: 0.0,
            // 初值 = 哨兵：没在动画（原版 `GraphwarButton::init` 写哨兵）。
            spin: SpinState::STOPPED,
            elapsed: 0.0,
            last_angle: 0.0,
            draw_calls: 0,
            process_calls: 0,
            arc_total: 0,
            finished_count: 0,
        }
    }

    /// `_process`：**本类新增的**（原版 2.0.60 里本模块只有 `draw`）。
    ///
    /// 只做两件事：推进 `elapsed`、要一次重画。**不**在这里判结束——原版是先画两弧、
    /// 再判 `angle > 2π`（`0x4d8485`/`0x4d84ed` 在前，`0x4d8503` 在后），所以哨兵写在
    /// [`IControl::draw`] 里，否则超时那一帧会被吞掉。
    fn process(&mut self, delta: f64) {
        self.process_calls += 1;
        if !self.spin.is_running() {
            return;
        }
        self.step_elapsed(delta);
        self.base_mut().queue_redraw();
    }

    /// `_draw`：原版 `GraphwarLongPressAnimation::draw`（vaddr `0x4d8100`，1232 B）。
    ///
    /// 顺序与指令一致：先判 `is_running`（`0x4d8110`，`nanos == 1e9` 直接返回），
    /// 再算 elapsed/angle 画**两条弧**（`0x4d8485` 黑、`0x4d84ed` 白），
    /// 最后判 `angle > 2π` 并写哨兵（`0x4d8503` / `0x4d850d`）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        let size = self.base().get_size();
        let pos = self.base().get_position();
        let geometry: Geometry = lpa::geometry(
            (pos.x, pos.y),
            (size.x, size.y),
            self.geometry_n as i32,
            self.geometry_c as f32,
        );

        let (secs, nanos) = split_secs(self.elapsed);
        let frame = lpa::draw_frame(self.spin, secs, nanos, geometry);
        let finished = matches!(frame, Frame::Finished(_));

        match frame {
            // `0x4d8115`：没在动画 ⇒ 一个图元都不发。
            Frame::Idle => {}
            Frame::Drawing(plan) | Frame::Finished(plan) => {
                for call in &plan {
                    let (center, radius, start, end, points, color, width, antialiased) =
                        arc_args(call);
                    // `base_mut()` 握着 `&mut self` ⇒ 计数只能在作用域外累加（文件头第 1 条）。
                    self.base_mut()
                        .draw_arc_ex(
                            Vector2::new(center.0, center.1),
                            radius,
                            start,
                            end,
                            points,
                            color_of(color),
                        )
                        .width(width)
                        .antialiased(antialiased)
                        .done();
                }
                self.arc_total += plan.len() as i64;
            }
        }

        if finished {
            // `0x4d850d`：只写 nanos = 1e9（secs 原样保留）。
            self.spin = self.spin.stopped();
            self.finished_count += 1;
        }
    }
}

#[godot_api]
impl GraphwarLongPressAnimation {
    /// 启动动画：等价 `GraphwarButton::button_down` 写 `Instant::now()`。
    ///
    /// 本类把"零点"当启动时刻（`started(0, 0)`），`elapsed` 清零后由 `_process` 累加。
    #[func]
    fn press(&mut self) {
        self.spin = SpinState::started(0, 0);
        self.elapsed = 0.0;
        self.last_angle = 0.0;
        self.base_mut().queue_redraw();
    }

    /// 手动推进 `delta` 秒（等价 `_process` 每帧做的那一段），然后要一次重画。
    ///
    /// **本类新增的观测口**：headless 的 `--script` 模式里帧间隔由引擎给，验收脚本没法
    /// 手算出"跑 37 帧正好 0.6 s"，所以把[推进]这一段单独暴露出来。
    /// 它调的是与 `_process` **同一个** `step_elapsed`，没有第二份算术。
    #[func]
    fn advance(&mut self, delta: f64) {
        self.step_elapsed(delta);
        self.base_mut().queue_redraw();
    }

    /// 推进 `elapsed` 并重算角度（`_process` 与 `advance` 共用这一段）。
    ///
    /// 负 `delta` 不倒退（引擎不会给，但别让累加器往回走）。
    fn step_elapsed(&mut self, delta: f64) {
        if !self.spin.is_running() {
            return;
        }
        self.elapsed += delta.max(0.0);
        let (secs, nanos) = split_secs(self.elapsed);
        self.last_angle = lpa::spin_angle(lpa::elapsed_secs(self.spin, secs, nanos));
    }

    /// 停掉动画：等价 `GraphwarButton::button_up` 写哨兵（`nanos = 1e9`）。
    #[func]
    fn release(&mut self) {
        self.spin = self.spin.stopped();
    }

    /// 现在还在动画里吗（`SpinState::is_running`：`nanos != 1e9`）。
    #[func]
    fn is_running(&self) -> bool {
        self.spin.is_running()
    }

    /// 本类自己累加的秒数。
    #[func]
    fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// 最近一次算出来的角度（弧度）；未启动时是 `0.0`。
    #[func]
    fn angle(&self) -> f64 {
        self.last_angle
    }

    /// 动画时长（秒）= `SPIN_LIMIT / SPIN_RATE` = `2π / 4π` = `0.5`。
    #[func]
    fn spin_duration_secs(&self) -> f64 {
        lpa::SPIN_DURATION_SECS
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// `_process` 被引擎调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// 累计发出过几条 `draw_arc`。
    #[func]
    fn arc_total(&self) -> i64 {
        self.arc_total
    }

    /// 动画**自己**结束过几次（`angle > 2π` 那条分支）。
    #[func]
    fn finished_count(&self) -> i64 {
        self.finished_count
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 时长 = `2π / 4π` = 0.5 s，与 500 ms 长按阈值**数值相同**（推断：同一个常量）。
    #[test]
    fn spin_duration_is_half_a_second() {
        assert!((lpa::SPIN_DURATION_SECS - 0.5).abs() < 1e-12);
        assert!((lpa::SPIN_LIMIT / lpa::SPIN_RATE - 0.5).abs() < 1e-12);
        assert!((lpa::SPIN_LIMIT - std::f64::consts::TAU).abs() < 1e-12);
    }

    /// 哨兵是 `nanos = 1e9`：`STOPPED` 不跑，`started` 跑，`stopped()` 只改 nanos。
    #[test]
    fn sentinel_nanos_marks_the_stopped_state() {
        assert!(!SpinState::STOPPED.is_running());
        let s = SpinState::started(7, 123);
        assert!(s.is_running());
        let off = s.stopped();
        assert!(!off.is_running());
        assert_eq!(off.secs, 7, "stopped() 只写 nanos");
        assert_eq!(off.nanos, lpa::SENTINEL_NANOS);
    }

    /// `_draw` 的两条弧：黑 `(-0.2 → angle+0.2)`、白 `(0 → angle)`，点数 30、不抗锯齿。
    ///
    /// 这是验收脚本里「arc_total == 2」那条断言在 Rust 侧的对应物。
    #[test]
    fn draw_plan_is_two_arcs_with_the_documented_arguments() {
        let g = lpa::geometry((10.0, 20.0), (60.0, 90.0), 0, 0.0);
        let plan = lpa::draw_plan(g, 1.5);
        assert_eq!(plan.len(), 2);
        for call in &plan {
            assert_eq!(call.esi, 0x1e, "点数是 0x1e = 30（观测）");
            assert_eq!(call.ecx, 0, "antialiased = 0（观测）");
            // xmm0/xmm1 = center、xmm2 = radius —— 两次绘制共用。
            assert_eq!(call.xmm0, g.v8);
            assert_eq!(call.xmm1, g.y);
            assert_eq!(call.xmm2, g.v10);
        }
        // 黑：角度两端各外扩 0.2，线宽按 0.2 那圈加粗。
        assert!((plan[0].xmm3 - (-0.2)).abs() < 1e-6);
        assert!((plan[0].xmm4 - 1.7).abs() < 1e-5);
        assert_eq!(plan[0].color, FxColor::BLACK);
        // 白：从 0 扫到 angle，线宽是 v4c。
        assert_eq!(plan[1].xmm3, 0.0);
        assert!((plan[1].xmm4 - 1.5).abs() < 1e-6);
        assert!((plan[1].xmm5 - g.v4c).abs() < 1e-6);
        assert_eq!(plan[1].color, FxColor::WHITE);
    }

    /// `arc_args` 是**纯搬运**：`DrawCall` 的 8 个槽一个不丢、一个不错位。
    #[test]
    fn arc_args_maps_every_slot_without_reordering() {
        let call = DrawCall {
            xmm0: 1.0,
            xmm1: 2.0,
            xmm2: 3.0,
            xmm3: 4.0,
            xmm4: 5.0,
            xmm5: 6.0,
            color: FxColor::WHITE,
            esi: 0x1e,
            ecx: 1,
        };
        let (center, radius, start, end, points, color, width, aa) = arc_args(&call);
        assert_eq!(center, (1.0, 2.0));
        assert_eq!(radius, 3.0);
        assert_eq!(start, 4.0);
        assert_eq!(end, 5.0);
        assert_eq!(points, 30);
        assert_eq!(color, FxColor::WHITE);
        assert_eq!(width, 6.0);
        assert!(aa, "ecx != 0 ⇒ antialiased");
    }

    /// `split_secs` 把累加的 f64 拆成 `(secs, nanos)`，且与 `elapsed_secs` 往返一致。
    #[test]
    fn split_secs_round_trips_through_elapsed_secs() {
        for e in [0.0_f64, 0.25, 0.5, 1.0, 1.75] {
            let (secs, nanos) = split_secs(e);
            let back = lpa::elapsed_secs(SpinState::started(0, 0), secs, nanos);
            assert!((back - e).abs() < 1e-6, "e={e} back={back}");
        }
        assert_eq!(split_secs(-1.0), (0, 0), "负值夹到 0");
        // 1e9 ns 是哨兵值，绝不能出现在"正在跑"的状态里。
        let (_, nanos) = split_secs(0.999_999_999_9);
        assert!(nanos < lpa::NANOS_PER_SEC, "nanos={nanos} 不得等于哨兵");
    }

    /// 0.5 s 正好越过终点：`angle = 0.5 × 4π = 2π`，而判据是**严格大于**。
    #[test]
    fn finish_threshold_is_strictly_greater_than_two_pi() {
        assert!(!lpa::is_finished(lpa::spin_angle(lpa::SPIN_DURATION_SECS)));
        assert!(lpa::is_finished(lpa::spin_angle(lpa::SPIN_DURATION_SECS + 1e-6)));
        let g = lpa::geometry((0.0, 0.0), (10.0, 30.0), 0, 0.0);
        assert!(matches!(
            lpa::draw_frame(SpinState::STOPPED, 0, 0, g),
            Frame::Idle
        ));
        assert!(matches!(
            lpa::draw_frame(SpinState::started(0, 0), 1, 0, g),
            Frame::Finished(_)
        ));
        assert!(matches!(
            lpa::draw_frame(SpinState::started(0, 0), 0, 0, g),
            Frame::Drawing(_)
        ));
    }
}
