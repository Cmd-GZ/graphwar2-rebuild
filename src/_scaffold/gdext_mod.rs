//! **不是原版的东西**：用**官方 gdext**（`godot` crate 0.5）重写的 GDExtension 绑定层。
//!
//! # 为什么会有这一份
//!
//! 原版 `libgraphwarrust.so` **自己就是 gdext 编出来的**：`.rodata` 里能找到
//! `/home/lucas/.cargo/git/checkouts/gdext-067f4b88e7bd088f/477501f/godot-core/src/registry/class.rs`
//! 这条路，所以 `IControl` / `GodotClass` / `ImplementsGodotVirtual` 这些都是 gdext 宏产物
//! （`docs/18-build.md` §10.1）。而我们现在那 576 行手写绑定（`gdext.rs` +
//! `gdext_class.rs` + `axis_drawer_class.rs` + `capi.rs`）只是**等价物**，不是原版形态。
//!
//! 本文件是"绑定层换回官方 gdext"这条路的最小可用骨架：类名、方法名、返回契约
//! **与手写层逐字一致**，所以 `client/` 一行都不用改
//! （`client/src/core/core_bridge.gd` 走的还是 `ClassDB.instantiate("GW2Rebuild")`）。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! ```
//!
//! 默认**关闭**：关闭时本 crate 仍然零依赖，`gdext_mod` 整个模块不参与编译。
//! 打开后手写入口 `_scaffold::gdext` 被 `#[cfg(not(feature = "gdext-bindings"))]` 关掉，
//! 因为两份入口都导出符号 `gdext_rust_init`，一个 `.so` 里只能有一个。
//!
//! # 这里**没有**第二份算法
//!
//! `ping` 之外的四个方法全部直接调 [`super::api`] 里的纯 Rust 函数
//! （`calculate_path` / `calculate_path_mode` / `hit_test` / `level_obstacles`），
//! 结果整形复用同一组 `*_pairs()`。本文件只做 **Godot 类型 ↔ Rust 类型** 的搬运，
//! 与手写 FFI 层搬的是同一批数。

use godot::classes::{Control, IControl, IRefCounted, RefCounted};
use godot::prelude::*;

use crate::axis_drawer::{self, AxisMode, Primitive};

use super::api::{self, hit_result_pairs, obstacle_pairs, result_pairs, ApiValue};

// ---------------------------------------------------------------------------
// 1. 类型搬运（Rust 域 → Godot 域）
// ---------------------------------------------------------------------------
//
// 手写 FFI 层是自己调 `variant_from_type_constructor` 一个个塞 Variant；
// gdext 这里全交给 `ToGodot`，一行 `dict.set(k, v)` 搞定。

/// `(f64, f64)` → Godot `Vector2`。
///
/// 注意精度：Godot 的 `Vector2` 是 **f32**（单精度构建），手写层写
/// `PACKED_VECTOR2_ARRAY` 时也是 f32，所以两边**同样**掉一遍精度，没有差别。
/// `hit_test` 的入参 `obstacles` / `soldiers` 故意用 `PackedFloat64Array`
/// 就是为了不在这条路上丢精度（`docs/120`）。
fn v2(x: f64, y: f64) -> Vector2 {
    Vector2::new(x as f32, y as f32)
}

/// [`ApiValue`] 落进 `Dictionary`。键的**顺序**由调用方的 `*_pairs()` 决定，这里不改。
fn set_value(dict: &mut VarDictionary, key: &str, value: &ApiValue) {
    match value {
        ApiValue::Bool(b) => dict.set(key, *b),
        ApiValue::F64(x) => dict.set(key, *x),
        ApiValue::Int(i) => dict.set(key, *i),
        ApiValue::Str(s) => dict.set(key, &GString::from(s.as_str())),
        ApiValue::Points(points) => {
            let mut arr = PackedVector2Array::new();
            for &(x, y) in points {
                arr.push(v2(x, y));
            }
            dict.set(key, &arr);
        }
        ApiValue::Vec2(x, y) => dict.set(key, v2(*x, *y)),
        ApiValue::IntArray(xs) => {
            let arr: Array<i64> = xs.iter().copied().collect();
            dict.set(key, &arr);
        }
        ApiValue::F64Array(xs) => {
            let arr: PackedFloat64Array = xs.iter().copied().collect();
            dict.set(key, &arr);
        }
    }
}

/// 把 `*_pairs()` 的产物整形成 GDScript 看到的 `Dictionary`。
fn dict_of(pairs: &[(&'static str, ApiValue)]) -> VarDictionary {
    let mut dict = VarDictionary::new();
    for (key, value) in pairs {
        set_value(&mut dict, key, value);
    }
    dict
}

// 注意：`dict_of()` / `set_value()` **没法写普通单测** —— `VarDictionary::new()`
// 要调引擎的构造函数，没有 Godot 进程就会 panic。所以这层只在真实引擎里验：
// `client/tools/gdext_check.gd` 的 24 条断言把每个键都读了一遍。

// ---------------------------------------------------------------------------
// 2. `GW2Rebuild`：算法出口单例（父类 `RefCounted`，与手写层同一个类名）
// ---------------------------------------------------------------------------

/// GDScript 侧的算法出口。**类名与方法名与手写注册逐字一致**：
///
/// ```text
/// GW2Rebuild.ping() -> String
/// GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float) -> Dictionary
/// GW2Rebuild.path_calculate_mode(expr: String, x0: float, y0: float, direction: float,
///                                mode: String) -> Dictionary
/// GW2Rebuild.hit_test(points: PackedVector2Array,
///                     obstacles: PackedFloat64Array,
///                     soldiers: PackedFloat64Array) -> Dictionary
/// GW2Rebuild.level_obstacles(level_index: int, seed: int) -> Dictionary
/// ```
///
/// `ping` 的返回值与手写层**不同**：手写层 `ping` 是空方法（`has_return_value = 0`），
/// 这里返回 `"pong:gdext"`。理由是可验证性——headless 里打印一次就能分辨
/// "加载的是 gdext 版还是手写版"，而客户端没有任何地方读 `ping` 的返回值。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GW2Rebuild {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GW2Rebuild {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GW2Rebuild {
    /// 存活探针。返回 `"pong:gdext"`（见类型文档里"为什么与手写层不同"）。
    #[func]
    fn ping(&self) -> GString {
        GString::from(format!("pong:{}", BINDING_TAG).as_str())
    }

    /// `y = f(x)` 弹道采样（`docs/116`）。
    #[func]
    fn path_calculate(&self, expr: GString, x0: f64, y0: f64, direction: f64) -> VarDictionary {
        let out = api::calculate_path(&expr.to_string(), x0, y0, direction);
        dict_of(&result_pairs(&out))
    }

    /// 带模式的弹道采样（`docs/138`）：`mode` 取 `"normal"` / `"ode"`（大小写不敏感）。
    #[func]
    fn path_calculate_mode(
        &self,
        expr: GString,
        x0: f64,
        y0: f64,
        direction: f64,
        mode: GString,
    ) -> VarDictionary {
        let out = api::calculate_path_mode(&expr.to_string(), x0, y0, direction, &mode.to_string());
        dict_of(&result_pairs(&out))
    }

    /// 命中判定（`docs/120`）。入参一律**棋盘坐标**，像素换算在内核里做一次。
    #[func]
    fn hit_test(
        &self,
        points: PackedVector2Array,
        obstacles: PackedFloat64Array,
        soldiers: PackedFloat64Array,
    ) -> VarDictionary {
        let pts: Vec<(f64, f64)> = points
            .as_slice()
            .iter()
            .map(|p| (p.x as f64, p.y as f64))
            .collect();
        // 步长 3 的约定由调用方保证；长度对不上时给 bad_obstacles / bad_soldiers，
        // 与手写层**同一套错误码**（那边也是先 `triples()` 再调同一个函数）。
        let out = match (triples(obstacles.as_slice()), triples(soldiers.as_slice())) {
            (Some(obs), Some(sol)) => api::hit_test(&pts, &obs, &sol),
            (None, _) => bad_hit(api::E_BAD_OBSTACLES),
            (_, None) => bad_hit(api::E_BAD_SOLDIERS),
        };
        dict_of(&hit_result_pairs(&out))
    }

    /// 关卡 → 障碍（`docs/126`）：`[x, y, size] * n`，与 `hit_test` 第 2 个入参同一种数组。
    #[func]
    fn level_obstacles(&self, level_index: i64, seed: i64) -> VarDictionary {
        let out = api::level_obstacles(level_index, seed);
        dict_of(&obstacle_pairs(&out))
    }

    /// 这个方法**不在**契约里，只是给自检脚本一个"我是谁"的口子，免得去猜。
    #[func]
    fn binding_version(&self) -> GString {
        GString::from(format!("gdext-bindings/{BINDING_TAG}").as_str())
    }
}

/// 绑定层自己的标签（`ping` 用它区分 gdext 版与手写版）。
const BINDING_TAG: &str = "gdext";

/// `[a, b, c, d, e, f]` → `[(a,b,c), (d,e,f)]`；长度不是 3 的倍数返回 `None`。
///
/// 与 `api.rs` 里的同名私有函数**语义相同**（那边是 `unsafe extern "C"` 回调的入口检查，
/// 这边是 `#[func]` 的入口检查）；判定本体仍然只有 `api::hit_test` 一份。
fn triples(flat: &[f64]) -> Option<Vec<(f64, f64, f64)>> {
    if flat.len() % 3 != 0 {
        return None;
    }
    Some(flat.chunks_exact(3).map(|c| (c[0], c[1], c[2])).collect())
}

/// `hit_test` 的入参形状不对时的返回值。
///
/// 手写层是在 `unsafe extern "C"` 回调里直接建这个结构；gdext 版没有那块内存，
/// 所以按 `HitOutcome` 的**公开字段**照抄一份（字段全 `pub`，`api.rs` 不需要改动）。
fn bad_hit(code: &'static str) -> api::HitOutcome {
    api::HitOutcome {
        ok: false,
        hits: Vec::new(),
        first_hit: -1,
        stop: (f64::NAN, f64::NAN),
        blocked: false,
        error: code,
    }
}

// ---------------------------------------------------------------------------
// 3. `AxisDrawer`：真节点类（父类 `Control`），带 `_draw` 虚方法
// ---------------------------------------------------------------------------
//
// 手写层（`axis_drawer_class.rs`，800 行）只注册了类 + 属性 + 4 个方法，
// **虚方法表一个都没接**——它不会画。这里换成 gdext 之后 `_draw` 只是一段
// 普通的方法覆写，虚方法派发由 gdext 的 `get_virtual_func` 表接管。
//
// 画什么仍然只有一份：`crate::axis_drawer::{AxisMode, axis_primitives}`（`AxisDrawer::draw`
// = vaddr 0x4e05b0 的图元级复原，`docs/16` §21）。本类只负责把 `Primitive` 翻译成
// `CanvasItem::draw_*` 调用。

/// 坐标轴 / 网格节点。
///
/// | 成员 | 类型 | 与手写层的对应 |
/// |---|---|---|
/// | 属性 `axis_mode` | `int` | 同名属性（`u8` 判别值收窄成 `i32` 存，见 [`clamp_mode`]） |
/// | `set_axis_mode` / `get_axis_mode` | 方法 | 同名方法（`#[export]` 自动生成的属性读写器） |
/// | `axis_line_count` | `() -> int` | 同名方法：跑 `axis_primitives` 数 `Line` 条数 |
/// | `axis_line_count_formula` | `() -> int` | 同名方法：闭式公式 `line_count` |
/// | `draw_call_count` / `drawn_line_total` | `() -> int` | **新增**：`_draw` 真被引擎调了几次、画了几条线 |
///
/// 最后两个是 gdext 才有的观测点：手写层没有虚方法表，也就没有"_draw 被调过"这件事。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct AxisDrawer {
    base: Base<Control>,
    /// `self + 0x18` 那个 `u8` 判别值的宿主（越界值按原版分支处理，见 [`AxisMode::from_raw`]）。
    #[export]
    axis_mode: i32,
    /// `_draw` 被引擎调用过几次。
    draw_call_count: i64,
    /// 最近一次 `_draw` 里发出的 `draw_line` 条数（累加到 `drawn_line_total`）。
    drawn_line_total: i64,
}

/// 棋盘尺寸（`BOARD_RECT.size`）：只影响 `axis_primitives` 里的像素换算，
/// **不影响线的条数**，所以没有 `Control::get_size` 也能数线。
const BOARD_SIZE: axis_drawer::Vec2 = axis_drawer::Vec2::new(50.0, 30.0);

/// 把任意 `i32` 收成判别值 `u8`：负数当 0，大于 255 当 255。
///
/// 与 `axis_drawer_class.rs::clamp_mode` **同一条规则**（原版判别值是 `u8` 且只比较
/// `==3` / `>=2` / `!=0`，所以收窄后语义不变；越界值在原版里**有行为**，不能拒绝）。
fn clamp_mode(value: i32) -> u8 {
    value.clamp(0, u8::MAX as i32) as u8
}

/// 某判别值下 `draw_line` 的条数（跑 `axis_primitives` 数 `Line`）。
fn line_primitive_count(raw_mode: u8) -> usize {
    axis_drawer::axis_primitives(AxisMode::from_raw(raw_mode), BOARD_SIZE)
        .iter()
        .filter(|p| matches!(p, Primitive::Line { .. }))
        .count()
}

/// `axis_drawer::Vec2` → Godot `Vector2`。
fn vec2_of(v: axis_drawer::Vec2) -> Vector2 {
    Vector2::new(v.x, v.y)
}

/// `axis_drawer::Color` → Godot `Color`。
fn color_of(c: axis_drawer::Color) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a)
}

#[godot_api]
impl IControl for AxisDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            axis_mode: AxisMode::NoAxis as i32,
            draw_call_count: 0,
            drawn_line_total: 0,
        }
    }

    /// `_draw`（原版 `AxisDrawer::draw` = vaddr `0x4e05b0`）：`draw_set_transform` 一次、
    /// `draw_rect` 一次、`draw_line` 0/2/20/102 次，顺序与二进制一致。
    fn draw(&mut self) {
        self.draw_call_count += 1;
        let lines = line_primitive_count(clamp_mode(self.axis_mode)) as i64;
        self.drawn_line_total += lines;

        // 原版用的是 `Control::get_size()`（`0x4e1743` 附近的 `divss`，除数 50）。
        let size = self.base().get_size();
        let primitives = axis_drawer::axis_primitives(
            AxisMode::from_raw(clamp_mode(self.axis_mode)),
            axis_drawer::Vec2::new(size.x, size.y),
        );

        let mut base = self.base_mut();
        for prim in &primitives {
            match *prim {
                Primitive::SetTransform { position, rotation, scale } => {
                    // Godot 4 的三个参数里后两个有默认值 ⇒ gdext 生成
                    // `draw_set_transform(pos)` + `draw_set_transform_ex(pos).rotation(..).scale(..)`
                    // 的 builder（`godot-codegen/src/generator/default_parameters.rs`）。
                    base.draw_set_transform_ex(vec2_of(position))
                        .rotation(rotation)
                        .scale(vec2_of(scale))
                        .done();
                }
                Primitive::Rect { rect, color, filled, width, antialiased } => {
                    base.draw_rect_ex(
                        Rect2::new(vec2_of(rect.pos), vec2_of(rect.size)),
                        color_of(color),
                    )
                    .filled(filled)
                    .width(width)
                    .antialiased(antialiased)
                    .done();
                }
                Primitive::Line { from, to, color, width, antialiased } => {
                    base.draw_line_ex(vec2_of(from), vec2_of(to), color_of(color))
                        .width(width)
                        .antialiased(antialiased)
                        .done();
                }
            }
        }
        // `draw_string` 一次都没有 —— 原版这个类不画文字（`docs/16` §21）。
    }
}

#[godot_api]
impl AxisDrawer {
    /// 枚举法数线：跑一遍 `axis_primitives`（与 `_draw` 用的是同一个函数）。
    #[func]
    fn axis_line_count(&self) -> i64 {
        line_primitive_count(clamp_mode(self.axis_mode)) as i64
    }

    /// 公式法数线：`axis_drawer::line_count`（闭式公式，用来对拍枚举法）。
    #[func]
    fn axis_line_count_formula(&self) -> i64 {
        axis_drawer::line_count(AxisMode::from_raw(clamp_mode(self.axis_mode))) as i64
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_call_count
    }

    /// 累计发出了多少条 `draw_line`。
    #[func]
    fn drawn_line_total(&self) -> i64 {
        self.drawn_line_total
    }
}

// ---------------------------------------------------------------------------
// 4. 入口
// ---------------------------------------------------------------------------
//
// 只写这几行就够 —— `gdext_rust_init` / 类注册 / 虚方法表 / 属性 / 方法的样板
// 全是宏生成的。手写层为了同样这点东西写了 576 行 ABI 搬运。

struct Gw2RebuildExtension;

#[gdextension]
unsafe impl ExtensionLibrary for Gw2RebuildExtension {}

// ---------------------------------------------------------------------------
// 5. 单测（**不碰引擎**：只测纯逻辑与契约键名）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_scaffold::api::{HIT_KEYS, OBSTACLE_KEYS, RESULT_KEYS};

    /// 判别值收窄：与 `axis_drawer_class.rs` 的同名规则一致（负数→0，>255→255）。
    #[test]
    fn clamp_mode_matches_handwritten_rule() {
        assert_eq!(clamp_mode(-1), 0);
        assert_eq!(clamp_mode(0), 0);
        assert_eq!(clamp_mode(3), 3);
        assert_eq!(clamp_mode(200), 200);
        assert_eq!(clamp_mode(1000), 255);
    }

    /// 枚举法与公式法在 0/1/2/3 与越界值上一致（`docs/16` §21 的 0/2/20/102）。
    #[test]
    fn enumerated_line_count_matches_formula() {
        for (raw, expected) in [(0u8, 0usize), (1, 2), (2, 20), (3, 102), (200, 20)] {
            assert_eq!(line_primitive_count(raw), expected, "raw={raw}");
            assert_eq!(
                axis_drawer::line_count(AxisMode::from_raw(raw)),
                expected,
                "formula raw={raw}"
            );
        }
    }

    /// 契约键名与 `api.rs` 里那几张表**同一份**（gdext 版不许自己再造一套）。
    #[test]
    fn dictionary_keys_reuse_the_api_contract() {
        assert_eq!(RESULT_KEYS, ["ok", "points", "offset", "error"]);
        assert_eq!(HIT_KEYS.len(), 7);
        assert_eq!(OBSTACLE_KEYS, ["ok", "obstacles", "error"]);
        // 键名唯一且非空：`dict_of()` 按顺序塞，重名会静默覆盖。
        let mut seen = std::collections::BTreeSet::new();
        for k in RESULT_KEYS.iter().chain(HIT_KEYS.iter()).chain(OBSTACLE_KEYS.iter()) {
            assert!(!k.is_empty());
            seen.insert(*k);
        }
        assert!(seen.contains("points") && seen.contains("stop"));
    }

    /// 内核入口还是那一个：gdext 版 `path_calculate` 与手写版调**同一个** `api::calculate_path`。
    #[test]
    fn gdext_path_uses_the_shared_kernel() {
        let direct = api::calculate_path("x", 0.0, 0.0, 1.0);
        let pairs = result_pairs(&direct);
        assert!(direct.ok, "内核本身要能算出 x 的弹道");
        assert!(direct.points.len() > 2, "点数应远大于 2，实际 {}", direct.points.len());
        // `result_pairs` 的四个键与 `dict_of` 塞进去的顺序一致。
        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs[0].0, "ok");
        assert_eq!(pairs[1].0, "points");
    }
}
