//! **不是原版的东西**：把复原的 [`crate::effect_drawer`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版的 `EffectDrawer` 是一个 `Control` 子类（`graphwarrust::effect_drawer::EffectDrawer`，
//! 类名字面量在 `effect_drawer::CLASS_NAME`），它自己**画**特效：`draw` 里发
//! `draw_set_transform` ×1 + `draw_circle` / `draw_polygon` / `draw_line`。
//!
//! `src/effect_drawer.rs` 已经把那套图元算术复原出来了（`frame_primitives` /
//! `effect_primitive` / `alpha_at` / `step_effects`），但它按约定**不碰引擎**（一行 Godot
//! 调用都没有）。本文件是「让那份图元清单**真的落到 `CanvasItem::draw_*` 上**」的那层薄壳：
//!
//! ```text
//! src/effect_drawer.rs                 纯逻辑：常量 / 相位跳表 / 变换 / 图元清单 / 子步积分
//! src/_scaffold/effect_drawer_class.rs ← 本文件：把 Primitive 翻译成 draw_* 并计数
//! ```
//!
//! 算术本体**只有一份**，仍在 `src/effect_drawer.rs` 里；本文件不复制任何算式
//! （唯一"新"的算术是 `as f32` 的窄化）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 图元的**种类与顺序** | `draw_set_transform` ×1、`draw_circle` ×4、`polygon` ×1、`draw_line` ×2（`docs/16` §18） | 同：直接遍历 [`effect_drawer::frame_primitives`] | **观测**（GOT 槽 + 全 `.text` 扫描） |
//! | 半径 / 颜色 / 透明度 | 0.05 / 0.06、四组基色、`max(0,(1−t)·0.9)` | 同（取自 `effect_drawer` 的常量与函数） | **观测** |
//! | 多边形**顶点** | 公式**未复原**（`effect_drawer` 模块文档 §5） | **不发明**顶点：纯逻辑给的 `points` 是空的 ⇒ 默认跳过；调用方用 [`EffectDrawer::set_polygon_placeholder`] 给才画 | 原版**未知**；给点才画是**我方选择** |
//! | 多边形颜色份数 | 每个顶点一份、**3 份同色**（`docs/16` §18 的「3 份同色」） | 纯逻辑给 1 个颜色 ⇒ 按**顶点数**复制 N 份（`N` 由调用方定，不硬编码 3） | **观测**（3 份）/ 复制规则**推断** |
//! | `draw_line` 的端点 | 公式**未复原**（`from == to == center`） | 照 `effect_primitive` 给的（退化线段：`draw_line` 照样被调到） | 端点**未知** |
//! | 当前 tick | `process` 从 `GraphwarModuleContainer` 的模块读 `[module+0x58]` | **自己数**：`step(delta)` 用 `effect_drawer::ticks_for_delta` 推进 | 推进规则**逐位**；"谁来喂 tick"**未接** |
//! | 撞障碍 | `0x4eb585` 调 `GraphwarObstacleManager::is_obstacle`；撞了 `vel = (0,0)` | 传 `|_| false`（= 场上没有障碍）——**没有任何障碍管理器** | 原版**观测**；本切片**缺失** |
//! | `process` 的 5 步 | 时间探针 → 读 tick → `retain` → `queue_redraw` → 收尾探针（`PROCESS_STEPS`） | 只做「推进 + `queue_redraw`」；探针与 `retain` 见 [`EffectDrawer::retain_alive`] | 步骤表**观测**；`retain` 判据**推断** |
//! | `ready` 的 2 个订阅 | `effect_info_event` / `tick_event`（`READY_SUBSCRIPTIONS`） | **没接**（`GraphwarBackend` 还没到节点层） | 名字与顺序**观测**；订阅**未做** |
//! | 元素容器 | `std::collections::HashMap`（迭代顺序**不可依赖**，`0x4e1830` 的 hashbrown 扫描） | `Vec`（顺序 = 插入顺序，可复现） | **化简**，不是原版形态 |
//!
//! ⇒ 这个类解决的是「**图元清单能不能真的变成 `draw_*` 调用**」，
//! **不是**"`EffectDrawer` 已经复原完了"（多边形顶点、线段端点、tick 来源都还缺）。
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
//! EffectDrawer : Control
//!   @export flip_y : bool = false        # 原版读 self+0x1f8 的位掩码（语义未知），这里由调用方给
//!
//!   spawn_effect(x, y, phase, duration_ticks) -> int   # 建一个 EffectData，返回 effect_count()
//!   clear_effects() -> void
//!   effect_count() -> int
//!   now_tick() -> int
//!   set_now_tick(t: int) -> void
//!   step(delta_seconds: float) -> int                  # 推进 tick + 物理子步（step_effects）
//!   retain_alive() -> int                              # 丢掉过期元素（推断判据），返回剩下几个
//!   effect_phase(i: int) -> int
//!   effect_alpha(i: int) -> float                      # 走 effect_drawer::alpha_at
//!   set_polygon_placeholder(points: PackedVector2Array) -> void
//!   draw_call_count() / process_call_count() / ready_call_count() -> int
//!   drawn_transform_total() / drawn_circle_total() / drawn_polygon_total() / drawn_line_total() -> int
//!   polygon_skipped_total() -> int                     # "这一帧本来要画多边形但没顶点"的次数
//! ```
//!
//! 后面那串计数是**观测点**：没有它们，headless 里分不清"引擎没调 `_draw`"和"调了但一个图元都没发"。
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §34.4 实测过，这里只记与本类有关的）
//!
//! 1. **`base_mut()` 是独占借用**（`WithBaseField::base_mut` 返回 `BaseMut<'_, Self>`，握着
//!    `&mut self`）⇒ 循环里既画东西又改 `self.xxx` 编译不过（`E0499`）。本类的做法：
//!    先在**局部变量**里累计计数，`base` 的作用域结束后再写回字段。
//! 2. **不用写 `set_process(true)`**：Godot 4.7 对实现了 `_process` 虚方法的扩展类自己开；
//!    原版 `process` 的证据里也没有这一句。
//! 3. **`draw_*` 只能在 `NOTIFICATION_DRAW` 里调**（引擎侧 `ERR_DRAW_GUARD`）⇒ 本类绝不从
//!    `#[func]` 里发图元，只发 `queue_redraw()`，等引擎回调 `_draw`。
//! 4. **`draw_polygon` 少于 3 个顶点会打印引擎错误**（`CanvasItem::draw_polygon` 的
//!    `ERR_FAIL_COND_MSG`）⇒ 顶点不足时本类**不调**（记到 `polygon_skipped_total`），
//!    而不是"调一下看它抱不抱怨"。

use godot::classes::{Control, IControl};
use godot::prelude::*;

use crate::effect_drawer::{self, Color as FxColor, EffectData, Phase, Primitive, Vec2D};

/// 把 `effect_drawer::Color`（RGBA 四个 f32）搬成 Godot 的 `Color`。
fn color_of(c: FxColor) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a)
}

/// 把 `(f32, f32)` 搬成 Godot 的 `Vector2`（Godot 4 默认单精度构建，不会掉精度）。
fn v2(p: (f32, f32)) -> Vector2 {
    Vector2::new(p.0, p.1)
}

/// 象限 1 的相位字节上限：`Phase::COUNT - 1`。越界的相位在原版里是跳表越界（未定义），
/// 本类**拒绝**而不是猜一个分支。
const MAX_PHASE_BYTE: i64 = (Phase::COUNT - 1) as i64;

/// `EffectDrawer`：把 [`effect_drawer::frame_primitives`] 的图元清单真的画出来的 `Control`。
///
/// 元素容器用 `Vec`（原版是 `HashMap`，迭代顺序不可依赖）；顺序 = `spawn_effect` 的调用顺序。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct EffectDrawer {
    base: Base<Control>,

    /// `draw` 时 y 轴是否翻转。
    ///
    /// 原版是 `self+0x1f8` 的位掩码（`and 0x101` / `test 0x10000` / `cmp 0x1000000`，
    /// 三个标志的**语义未知**，见 [`effect_drawer::draw_transform`] 的文档）⇒ 这里由调用方给。
    #[export]
    flip_y: bool,

    /// 特效元素。原版是 `HashMap`（`EffectDrawer::process` 用 `Vec::retain` 清理它）。
    effects: Vec<EffectData>,

    /// 当前 tick。原版从 `GraphwarModuleContainer` 里那个模块的 `+0x58` 读；本类自己数。
    now_tick: u32,

    /// 多边形占位顶点（`effect_drawer` 模块文档 §5：顶点公式未复原）。
    ///
    /// **顺序无关的调试口**：空 = 不画多边形（原版的几何我们不知道）；给了 ≥3 个点才画。
    polygon_placeholder: Vec<(f32, f32)>,

    /// `_draw` 被引擎调用过几次（观测点）。
    draw_calls: i64,
    /// `_process` 被引擎调用过几次（观测点）。
    process_calls: i64,
    /// `_ready` 被引擎调用过几次（观测点）。
    ready_calls: i64,

    /// 累计发出的 `draw_set_transform` 条数。
    transform_total: i64,
    /// 累计发出的 `draw_circle` 个数。
    circle_total: i64,
    /// 累计发出的 `draw_polygon` 个数。
    polygon_total: i64,
    /// 累计发出的 `draw_line` 条数。
    line_total: i64,
    /// 「这一帧本来要画多边形、但没有顶点可用」的次数（原版这里**会**画一个三角形）。
    polygon_skipped: i64,
}

#[godot_api]
impl IControl for EffectDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            // 原版那三个标志的语义未知 ⇒ 默认不翻（y 轴朝下，与 `axis_drawer` 的默认一致）。
            flip_y: false,
            effects: Vec::new(),
            now_tick: 0,
            polygon_placeholder: Vec::new(),
            draw_calls: 0,
            process_calls: 0,
            ready_calls: 0,
            transform_total: 0,
            circle_total: 0,
            polygon_total: 0,
            line_total: 0,
            polygon_skipped: 0,
        }
    }

    /// `_ready`：原版在这里订 2 个后端事件（[`effect_drawer::READY_SUBSCRIPTIONS`]：
    /// `effect_info_event` 先、`tick_event` 后）。
    ///
    /// 本切片**只记数**：要订事件得有 `GraphwarBackend`，那个模块还没接到节点层。
    fn ready(&mut self) {
        self.ready_calls += 1;
    }

    /// `_process`：原版做 5 步（[`effect_drawer::PROCESS_STEPS`]）；本类做「推进 + 要重画」。
    ///
    /// 少了的三步各自有理由：两个时间探针只是 `_print` 超时（`TIME_PROBE_*`），
    /// `RetainAlive` 的判据是**推断**（见 [`EffectDrawer::retain_alive`]，单独暴露成方法）。
    fn process(&mut self, delta: f64) {
        self.process_calls += 1;
        self.step(delta);
        if !self.effects.is_empty() {
            // 原版 `0x4e35ec` 调 `CanvasItem::queue_redraw`（槽 `0x9baa60`）。
            self.base_mut().queue_redraw();
        }
    }

    /// `_draw`：原版 `EffectDrawer::draw`（vaddr `0x4e1520`）。
    ///
    /// 顺序与二进制一致：**先一条** `draw_set_transform`（`0x4e17c0`），然后每个存活元素一条图元。
    /// 图元清单整个来自 [`effect_drawer::frame_primitives`]，本函数只做「Rust 图元 → `draw_*`」。
    fn draw(&mut self) {
        self.draw_calls += 1;

        // 尺寸先在**语句内**取完（`base()` 借的是 `&self`，出语句就结束）。
        let size = self.base().get_size();
        let mut primitives = effect_drawer::frame_primitives(
            &self.effects,
            self.now_tick,
            self.flip_y,
            (size.x, size.y),
        );

        // 顶点公式未复原 ⇒ 纯逻辑给的是空 `points`。调用方给了占位顶点才补上（见文件头对照表）。
        let placeholder = self.polygon_placeholder.clone();
        for prim in primitives.iter_mut() {
            if let Primitive::Polygon { points, .. } = prim {
                if points.is_empty() {
                    *points = placeholder.clone();
                }
            }
        }

        // 计数先在局部变量里累计：`base_mut()` 握着 `&mut self`，那段里碰不到 `self.xxx`
        // （`E0499`，见文件头第 1 条）。
        let mut transform = 0i64;
        let mut circle = 0i64;
        let mut polygon = 0i64;
        let mut line = 0i64;
        let mut skipped = 0i64;

        {
            let mut base = self.base_mut();
            for prim in &primitives {
                match prim {
                    Primitive::SetTransform(t) => {
                        transform += 1;
                        // Godot 4 的后两个参数有默认值 ⇒ gdext 生成 `*_ex(...)` builder。
                        base.draw_set_transform_ex(v2(t.position))
                            .rotation(t.rotation)
                            .scale(v2(t.scale))
                            .done();
                    }
                    Primitive::Circle {
                        center,
                        radius,
                        color,
                        filled,
                        width,
                        antialiased,
                    } => {
                        circle += 1;
                        base.draw_circle_ex(v2(*center), *radius, color_of(*color))
                            .filled(*filled)
                            .width(*width)
                            .antialiased(*antialiased)
                            .done();
                    }
                    Primitive::Line {
                        from,
                        to,
                        color,
                        width,
                        antialiased,
                    } => {
                        line += 1;
                        // 端点公式未复原 ⇒ `from == to == center`（退化线段）。原版**照样**
                        // 调一次 `draw_line`（`0x4e1520` 的 2 个 `0x9bc228` 调用点），不跳过。
                        base.draw_line_ex(v2(*from), v2(*to), color_of(*color))
                            .width(*width)
                            .antialiased(*antialiased)
                            .done();
                    }
                    Primitive::Polygon { points, colors } => {
                        // `CanvasItem::draw_polygon` 要求 ≥3 个点，否则打印引擎错误
                        // ⇒ 顶点没复原时**不调**（文件头第 4 条）。
                        if points.len() < 3 || colors.is_empty() {
                            skipped += 1;
                            continue;
                        }
                        polygon += 1;
                        let mut pts = PackedVector2Array::new();
                        for p in points {
                            pts.push(v2(*p));
                        }
                        // 单个颜色 → 每个顶点一份（原版观测到「3 份同色」；份数 = 顶点数，
                        // 是**推断**，不硬编码 3）。见文件头对照表。
                        let mut cols = PackedColorArray::new();
                        for i in 0..points.len() {
                            let c = colors[i % colors.len()];
                            cols.push(color_of(c));
                        }
                        base.draw_polygon(&pts, &cols);
                    }
                }
            }
        }

        self.transform_total += transform;
        self.circle_total += circle;
        self.polygon_total += polygon;
        self.line_total += line;
        self.polygon_skipped += skipped;
    }
}

#[godot_api]
impl EffectDrawer {
    /// 建一个特效元素（`EffectData::new`：位置 + 起始 tick + 持续 tick + 相位）。
    ///
    /// * `start_tick` 取**当前** tick（原版的起始 tick 从 tick 模块来，本类自己数）；
    /// * `phase` 取 `0..=6`（[`Phase::COUNT`]），越界返回 `-1` 且**不建**——原版跳表越界是
    ///   未定义行为，本类不猜分支。
    ///
    /// 返回建完之后 `effect_count()`。
    #[func]
    fn spawn_effect(&mut self, x: f64, y: f64, phase: i64, duration_ticks: i64) -> i64 {
        if !(0..=MAX_PHASE_BYTE).contains(&phase) {
            godot_warn!("EffectDrawer::spawn_effect 相位越界 {phase}（有效 0..=6）");
            return -1;
        }
        let Some(phase) = Phase::from_byte(phase as u8) else {
            return -1;
        };
        let duration_ticks = duration_ticks.clamp(0, u32::MAX as i64) as u32;
        let data = EffectData::new(
            Vec2D::new(x, y),
            self.now_tick,
            duration_ticks,
            phase,
        );
        // 原版 `EffectData` 的 `vel` 初值**未知**（`EffectData::new` 的字段初值未复原），
        // 纯逻辑给的是 (0,0) ⇒ 物理子步不会移动它。这里**照抄纯逻辑**，不加自己的初值。
        self.effects.push(data);
        self.base_mut().queue_redraw();
        self.effects.len() as i64
    }

    /// 清空所有特效元素。
    #[func]
    fn clear_effects(&mut self) {
        self.effects.clear();
        self.base_mut().queue_redraw();
    }

    /// 当前持有几个特效元素。
    #[func]
    fn effect_count(&self) -> i64 {
        self.effects.len() as i64
    }

    /// 当前 tick（原版从模块 `+0x58` 读，本类自己数）。
    #[func]
    fn now_tick(&self) -> i64 {
        self.now_tick as i64
    }

    /// 直接把当前 tick 设成某个值（验收脚本用它对齐期望值）。
    #[func]
    fn set_now_tick(&mut self, tick: i64) {
        self.now_tick = tick.clamp(0, u32::MAX as i64) as u32;
        self.base_mut().queue_redraw();
    }

    /// 推进一帧：`ticks = clamp(60·delta, 0, u32::MAX)`，然后跑
    /// [`effect_drawer::step_effects`]（快路径 / 1/600 s 子步两分支都在那个函数里）。
    ///
    /// **没有障碍管理器** ⇒ `is_obstacle` 恒 `false`（文件头对照表）。
    /// 返回推进后的 `now_tick()`。
    #[func]
    fn step(&mut self, delta_seconds: f64) -> i64 {
        self.now_tick = self
            .now_tick
            .saturating_add(effect_drawer::ticks_for_delta(delta_seconds));
        let now = self.now_tick;
        effect_drawer::step_effects(&mut self.effects, now, delta_seconds, |_pos| false);
        now as i64
    }

    /// 丢掉过期元素，返回剩下几个。
    ///
    /// 原版 `process` 的第 3 步是 `Vec::retain`（`0x4e3397` 前后的 `call 0x3cb080`），
    /// 但**判据没读到**（`effect_drawer` 模块文档只记了"丢掉过期元素"）。
    /// 本类用 [`EffectData::is_alive_at`]（= `draw` 判活的同一条判据）⇒ **推断**，
    /// 所以它**不**在 `process` 里自动跑，而是单独暴露给调用方。
    #[func]
    fn retain_alive(&mut self) -> i64 {
        let now = self.now_tick;
        self.effects.retain(|e| e.is_alive_at(now));
        self.effects.len() as i64
    }

    /// 第 `index` 个元素现在的相位字节（越界返回 `-1`）。
    #[func]
    fn effect_phase(&self, index: i64) -> i64 {
        match self.effects.get(index.max(0) as usize) {
            Some(e) => e.phase as i64,
            None => -1,
        }
    }

    /// 第 `index` 个元素此刻的透明度（走 [`effect_drawer::alpha_at`]；越界返回 `-1.0`）。
    ///
    /// 相位 6 是特例（固定 `ALPHA_PHASE6`），相位 1 走 `polygon_alpha` ⇒ 这两种情况
    /// 这里返回 `-1.0`，让调用方知道"这个相位不看 `alpha_at`"。
    #[func]
    fn effect_alpha(&self, index: i64) -> f64 {
        let Some(e) = self.effects.get(index.max(0) as usize) else {
            return -1.0;
        };
        match e.phase {
            Phase::CircleDark | Phase::Polygon => -1.0,
            _ => effect_drawer::alpha_at(self.now_tick, e.start_tick, e.duration_ticks) as f64,
        }
    }

    /// 给多边形相位一组**占位顶点**（棋盘坐标）。
    ///
    /// 原版的顶点公式**没复原**（`effect_drawer` 模块文档 §5：只读到两组点各乘 `0.2`）。
    /// 本类绝不发明几何 ⇒ 默认不画；调用方（验收脚本 / 未来的几何复原）给了点才画。
    /// 少于 3 个点等价于清空。
    #[func]
    fn set_polygon_placeholder(&mut self, points: PackedVector2Array) {
        self.polygon_placeholder = if points.len() < 3 {
            Vec::new()
        } else {
            points.as_slice().iter().map(|p| (p.x, p.y)).collect()
        };
        self.base_mut().queue_redraw();
    }

    /// 当前有几条占位顶点（0 = 不画多边形）。
    #[func]
    fn polygon_placeholder_count(&self) -> i64 {
        self.polygon_placeholder.len() as i64
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

    /// `_ready` 被引擎调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// 累计发出过几条 `draw_set_transform`。
    #[func]
    fn drawn_transform_total(&self) -> i64 {
        self.transform_total
    }

    /// 累计发出过几个 `draw_circle`。
    #[func]
    fn drawn_circle_total(&self) -> i64 {
        self.circle_total
    }

    /// 累计发出过几个 `draw_polygon`。
    #[func]
    fn drawn_polygon_total(&self) -> i64 {
        self.polygon_total
    }

    /// 累计发出过几条 `draw_line`。
    #[func]
    fn drawn_line_total(&self) -> i64 {
        self.line_total
    }

    /// 「本来要画多边形、但没有顶点」的次数。
    #[func]
    fn polygon_skipped_total(&self) -> i64 {
        self.polygon_skipped
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」，不发明第二份）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/draw_classes_check.gd`（headless，真引擎）。
// 这里钉的是三件容易悄悄跑偏的事：相位表、alpha 的分档、几何常量的来源。

#[cfg(test)]
mod tests {
    use super::*;

    /// 相位表就是 `effect_drawer::Phase` 的 7 个值，`MAX_PHASE_BYTE` 与它同步。
    #[test]
    fn phase_range_matches_the_pure_module() {
        assert_eq!(Phase::COUNT, 7);
        assert_eq!(MAX_PHASE_BYTE, 6);
        for b in 0..=MAX_PHASE_BYTE as u8 {
            assert!(Phase::from_byte(b).is_some(), "相位 {b} 应该有效");
        }
        assert!(Phase::from_byte(7).is_none(), "相位 7 在跳表外");
        assert!(Phase::from_byte(255).is_none());
    }

    /// 半径只有 0.05 / 0.06 两个值，且**圆相位**才有半径（多边形 / 线段没有）。
    #[test]
    fn circle_radii_come_from_the_two_constants() {
        assert_eq!(
            Phase::CircleSmall.circle_radius(),
            Some(effect_drawer::RADIUS_SMALL)
        );
        for p in [Phase::CircleLargeA, Phase::CircleLargeB, Phase::CircleDark] {
            assert_eq!(p.circle_radius(), Some(effect_drawer::RADIUS_LARGE));
        }
        for p in [Phase::Polygon, Phase::LineA, Phase::LineB] {
            assert_eq!(p.circle_radius(), None);
        }
        assert!((effect_drawer::RADIUS_SMALL - 0.05).abs() < 1e-9);
        assert!((effect_drawer::RADIUS_LARGE - 0.06).abs() < 1e-9);
    }

    /// `_draw` 真的是「1 条变换 + 每个存活元素 1 条图元」，而且相位 0 给的是圆。
    ///
    /// 这是验收脚本里「circle_total == 1」那条断言在 Rust 侧的对应物。
    #[test]
    fn frame_primitives_is_one_transform_plus_one_per_effect() {
        let e = EffectData::new(Vec2D::new(5.0, 5.0), 0, 60, Phase::CircleSmall);
        let prims = effect_drawer::frame_primitives(&[e], 0, false, (500.0, 300.0));
        assert_eq!(prims.len(), 2, "1 条 SetTransform + 1 条图元");
        assert!(matches!(prims[0], Primitive::SetTransform(_)));
        let Primitive::Circle { center, radius, color, .. } = prims[1] else {
            panic!("相位 0 应该是圆，实际 {:?}", prims[1]);
        };
        assert_eq!(center, (5.0, 5.0));
        assert_eq!(radius, effect_drawer::RADIUS_SMALL);
        // `alpha_at(0, 0, 60)` = 0.9（`now <= start` 那条快路）
        assert!((color.a - 0.9).abs() < 1e-6, "a={}", color.a);
    }

    /// 过期元素不再进图元清单（`is_alive_at` = `now <= start + duration`）。
    #[test]
    fn expired_effects_do_not_emit_primitives() {
        let e = EffectData::new(Vec2D::new(0.0, 0.0), 0, 10, Phase::CircleSmall);
        assert!(!effect_drawer::frame_primitives(&[e], 11, false, (500.0, 300.0))
            .iter()
            .any(|p| matches!(p, Primitive::Circle { .. })));
        assert!(effect_drawer::frame_primitives(&[e], 10, false, (500.0, 300.0))
            .iter()
            .any(|p| matches!(p, Primitive::Circle { .. })));
    }

    /// 多边形相位在纯逻辑里**故意没有顶点**（本类据此记 `polygon_skipped`）。
    #[test]
    fn polygon_phase_has_no_vertices_to_draw() {
        let e = EffectData::new(Vec2D::new(5.0, 5.0), 0, 60, Phase::Polygon);
        let prims = effect_drawer::frame_primitives(&[e], 0, false, (500.0, 300.0));
        let Primitive::Polygon { points, colors } = &prims[1] else {
            panic!("相位 1 应该是多边形，实际 {:?}", prims[1]);
        };
        assert!(points.is_empty(), "顶点公式未复原 ⇒ 故意留空");
        assert_eq!(colors.len(), 1, "纯逻辑给 1 个颜色，本类按顶点数复制");
    }

    /// 线段相位给的是**退化**线段（`from == to == center`）—— 原版端点未复原。
    #[test]
    fn line_phase_is_a_degenerate_segment() {
        let e = EffectData::new(Vec2D::new(3.0, -4.0), 0, 60, Phase::LineA);
        let prims = effect_drawer::frame_primitives(&[e], 0, false, (500.0, 300.0));
        let Primitive::Line { from, to, .. } = &prims[1] else {
            panic!("相位 4 应该是线段，实际 {:?}", prims[1]);
        };
        assert_eq!(from, to, "端点公式未复原 ⇒ 两端同点");
        assert_eq!(*from, (3.0, -4.0));
    }

    /// `ticks_for_delta`：60 tick = 1 秒，1/60 s 一帧 = 1 tick（`60 × 0.016666…` 截断后是 0 还是 1，
    /// 由纯逻辑决定，别在这里"修"它）。
    #[test]
    fn step_advances_ticks_by_the_pure_rule() {
        assert_eq!(effect_drawer::ticks_for_delta(1.0), 60);
        assert_eq!(effect_drawer::ticks_for_delta(0.5), 30);
        assert_eq!(effect_drawer::ticks_for_delta(-1.0), 0, "负数夹到 0");
        // ⚠️ `60.0 × (1.0/60.0)` 在 f64 里**不是**精确的 1（实测截断后得 **1**），所以
        // "一帧 = 几 tick"由浮点舍入决定，靠整除猜会错。这里只钉住它落在 `0..=1` 里。
        let per_frame = effect_drawer::ticks_for_delta(1.0 / 60.0);
        assert!(per_frame <= 1, "per_frame={per_frame}");
        // 1/600 s（物理子步）比一帧还小一个数量级 ⇒ 60·(1/600) = 0.1 ⇒ 截断成 0。
        assert_eq!(
            effect_drawer::ticks_for_delta(effect_drawer::FIXED_STEP_SECONDS),
            0
        );
    }
}
