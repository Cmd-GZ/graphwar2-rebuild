//! **不是原版的东西**：把复原的 [`crate::graphwar_function_input`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarFunctionInput` 是 **`LineEdit` 的 Rust 子类**（`ILineEdit` + `GodotClass` +
//! `ImplementsGodotApi` + `ImplementsGodotVirtual`）：**它不是 `.tscn` 里的 LineEdit 节点**，
//! 它自己就是输入框，文字状态在 Rust 侧（`docs/06-verification.md` §15.3 的运行时观测）。
//! `src/graphwar_function_input.rs` 已经把那 17 个函数里**可归属**的部分复原出来了 ——
//! 尤其是"输入框只做 `text.replace(' ',"")`"这一条 —— 但它按约定**不碰引擎**。
//! 本文件是那一层的薄壳：
//!
//! ```text
//! src/graphwar_function_input.rs            纯逻辑：strip_spaces / COMPOSED_FOLDS / intent_for_key
//! src/_scaffold/graphwar_function_input_class.rs   ← 本文件：connect 三条原生信号、插字、发信号
//! ```
//!
//! 预处理与折叠表**只有一份**，仍然在纯逻辑文件里；本文件不复制任何字符表。
//!
//! # 本轮要验的三种机制（结论见 `docs/16-client.md` §39）
//!
//! | # | 机制 | 原版形态 | 本文件怎么用 |
//! |---|---|---|---|
//! | 1 | **自造 `#[signal]`** | 无 | `#[signal] expression_changed(expression)` / `function_submitted(text)`，用**裸** `emit_signal` 发（与 `GraphwarButton` 的类型安全生成器对照） |
//! | 2 | **连引擎原生信号** | `ready` 里 3 条 `connect`（[`graphwar_function_input::SIGNAL_WIRING`]） | `size_changed`/`text_changed`/`text_submitted` → `on_size_changed`/`on_text_change`/`on_text_submitted` |
//! | 3 | **外部注册的闭包表** | **单个** `dyn` 回调（`[self+0x68]`，`0x4f425c`），按名字调 `"fire"`（`0x4f4269`） | 字段写 `Option<Box<dyn FnMut(&str)>>`；另存 `Vec<Callable>` 给 GDScript |
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 / trait | `LineEdit` + `ILineEdit` | 同 | **观测** |
//! | `ready` 的 3 条连接 | `size_changed`/`text_changed`/`text_submitted` → 三个 `#[func]` | 同（名字逐字取自 [`graphwar_function_input::SIGNAL_WIRING`]） | **观测** |
//! | 文本预处理 | `force_update_cb` 里 `str::replace(' ', "")` | 调 [`graphwar_function_input::strip_spaces`]（同一份） | **观测** |
//! | `pi`/`π` → `n`、小写化 | **不在输入框**，在 `MathFunction::new`（解析器） | **本类不做**（也不该做） | **观测** |
//! | `gui_input` 的 28 条折叠 | 跳转表 `0x1c3d5c`，198 项里 28 项不落默认 | 调 [`graphwar_function_input::intent_for_key`]（同一张表） | **观测** |
//! | `'^'` 的附加条件 | `0x4feeeb`–`0x4fef06` 三次读引擎属性，**语义未知** | `#[export] caret_condition : bool` 显式暴露，默认 `false` | 条件**未知**（缺那三次读的语义） |
//! | token 边界（`move_caret_*` / `delete_token_*` 的 N） | 内联的 `0x3e90f0` 调用序列，**规则未解** | **没实现**（只提供 `buffer` 级别的 `#[func]` 走纯逻辑的 `Buffer`） | **未解**（`graphwar_function_input::UNKNOWNS` 第 1 条） |
//! | `process` 那 6.4 KB | 两次 `call("force_update_cb")` + 与 `graphwar_game_client` 交互 | 只做"每帧 `force_update_cb`"，game_client 那半**没接** | 交互**未做** |
//! | 提交路径 | `[+0x68]` 的 dyn 回调 `"fire"`，文本**原样** | 同（`Option<Box<dyn FnMut(&str)>>`），文本原样 | **观测** |
//!
//! # 怎么打开 / 契约
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/signal_classes_check.gd
//! ```
//!
//! ```text
//! GraphwarFunctionInput : LineEdit
//!   @export caret_condition : bool           # '^' 那个未知条件的替身
//!
//!   signal expression_changed(expression: String)   # 自造：去空格之后
//!   signal function_submitted(text: String)         # 自造：提交（文本原样）
//!
//!   on_text_change(new_text) / on_text_submitted(text) / on_size_changed()
//!   force_update_cb() -> String               # 读 get_text() → 去空格 → 变了就发信号
//!   expression() -> String                    # 最近一次去空格的结果
//!   simulate_key(unicode: int, caret_condition: bool) -> String   # 28 条折叠的可测入口
//!   subscribe(cb: Callable) -> int            # 替代方案
//!   subscribe_native_probe() -> bool          # 原版形态：设那个单 dyn 回调
//!   gui_input_call_count() / text_change_count() / submit_count() -> int
//!   size_changed_count() / ready_call_count() / process_call_count() -> int
//!   expression_emit_count() / submit_emit_count() -> int
//!   fire_captured() / callable_captured() / fire_log() -> String
//!   insert_count() -> int
//! };
//! ```
//!
//! # 必须知道的三条（本轮实测）
//!
//! 1. **`--script` 里没有真实按键** ⇒ `gui_input` 不会被引擎调到。可测的是
//!    [`GraphwarFunctionInput::simulate_key`]（走**同一个** `apply_key`）。
//! 2. **信号名与处理函数名不同名**（`text_changed` → `on_text_change`），所以这里的
//!    `connect` 必须给两个不同的名字 —— 与 `GraphwarButton` 那条"同名"正好对照。
//! 3. **`Callable` 不能当 `#[export]` 属性**：gdext 的 `impl_property_by_godot_convert!`
//!    对 `Callable`/`Signal`/`Rid` 只生成 `SimpleVar`、**不生成 `Export`**
//!    （`godot-core-0.5.5/src/registry/property/mod.rs` 的 `no_export` 分支）；
//!    想把它暴露给编辑器只能加 `#[var]` 而不是 `#[export]`。见 `docs/16-client.md` §39.6。

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::{ILineEdit, InputEvent, InputEventKey, LineEdit};
use godot::prelude::*;

use crate::graphwar_function_input::{self as finput};

/// 最多留多少条派发/提交流水。
const LOG_LIMIT: usize = 32;

/// 函数输入框。
///
/// 文字状态在**引擎**（`LineEdit` 自己的 `text`），Rust 侧只留"去空格之后的结果"
/// [`GraphwarFunctionInput::expression`] —— 这正是原版的分工：`force_update_cb`
/// 读引擎文本、去掉空格、再把结果送去算路径（`0x4f28be` 的 `str::replace`）。
#[derive(GodotClass)]
#[class(base = LineEdit)]
pub struct GraphwarFunctionInput {
    base: Base<LineEdit>,

    /// `'^'` 那个**未知**附加条件（`0x4fee94`/`0x4feea5`/`0x4feee3` 三次引擎属性读取）的替身。
    ///
    /// 默认 `false` = "按未知条件不成立处理"；这个默认值**不是**从原版读出来的。
    #[export]
    caret_condition: bool,

    /// 最近一次去空格之后的表达式（原版 `force_update_cb` 的中间结果）。
    expression: GString,

    /// **机制 3 的原版形态**：`[self+0x68]` 是**一个** dyn 回调（`mov [rdi+0x68],%rdi`）。
    ///
    /// 原版调它时按名字传 `"fire"`（`lea "fire"(4)` + `call *0x20(%rax)`）—— 名字派发是
    /// gdext 弱引用 trait 对象的形态；本文件只保留"一个可换的回调"这一条语义。
    fire_native: Option<Box<dyn FnMut(&str)>>,

    /// 探针闭包写到这儿（闭包不能借 `self`，见 `graphwar_button_class.rs` §坑 3）。
    native_capture: Rc<RefCell<Vec<String>>>,

    /// **机制 3 的替代方案**：GDScript 只能给 `Callable`，而且允许登记多个（我方选择）。
    fire_callables: Vec<Callable>,

    /// `Callable` 的到货记录。
    callable_capture: Rc<RefCell<Vec<String>>>,

    ready_calls: i64,
    process_calls: i64,
    text_change_calls: i64,
    submit_calls: i64,
    size_changed_calls: i64,
    force_update_calls: i64,
    gui_input_calls: i64,
    insert_count: i64,
    expression_emits: i64,
    submit_emits: i64,
    emit_errors: Vec<String>,
    /// `_ready` 里连不上的信号（`connect` 的返回码）。原版第 3 条 `size_changed` 会落这里。
    connect_errors: Vec<String>,
    /// `_ready` 里真连上几条（期望 2 —— 见 [`GraphwarFunctionInput::connect_errors`]）。
    wired: i64,
}

#[godot_api]
impl ILineEdit for GraphwarFunctionInput {
    fn init(base: Base<LineEdit>) -> Self {
        Self {
            base,
            caret_condition: false,
            expression: GString::new(),
            fire_native: None,
            native_capture: Rc::new(RefCell::new(Vec::new())),
            fire_callables: Vec::new(),
            callable_capture: Rc::new(RefCell::new(Vec::new())),
            ready_calls: 0,
            process_calls: 0,
            text_change_calls: 0,
            submit_calls: 0,
            size_changed_calls: 0,
            force_update_calls: 0,
            gui_input_calls: 0,
            insert_count: 0,
            expression_emits: 0,
            submit_emits: 0,
            emit_errors: Vec::new(),
            connect_errors: Vec::new(),
            wired: 0,
        }
    }

    /// `_ready`（原版 `0x4fd1b0`）：**机制 2** —— 三条 `LineEdit` 原生信号接回自己的方法。
    ///
    /// 原版三段的结构一样（`Callable::from_object_method(self, handler)` +
    /// `StringName::from(signal)` + `connect`），指令地址见
    /// [`graphwar_function_input::SIGNAL_WIRING`] 的文档。
    fn ready(&mut self) {
        self.ready_calls += 1;

        let myself = self.to_gd();
        for (signal, handler) in finput::SIGNAL_WIRING {
            let callable = myself.callable(handler);
            let err = self.base_mut().connect(signal, &callable);
            if err == godot::global::Error::OK {
                self.wired += 1;
            } else {
                // ⚠️ **实测**：`size_changed` 在 Godot 4.6/4.7 的 `Control`/`LineEdit` 上
                // **不是信号**（只有 `resized`）⇒ 这一条必然 `ERR_INVALID_PARAMETER`。
                // 原版的字面量确实是 `size_changed`（`.rodata 0x1c46d2`，12 字节，逐字节核过），
                // 所以这是**原版自己的**问题，不是本文件的笔误。见 `docs/16-client.md` §39.5。
                godot_warn!("GraphwarFunctionInput::ready 连接 {signal} 失败: {err:?}");
                self.connect_errors.push(format!("{signal}: {err:?}"));
            }
        }
    }

    /// `_process`：原版 `0x4fd4a0`（6.4 KB）里**已确认**的那部分 —— 两次
    /// `call("force_update_cb")`。与 `game_client` 的数据流（`docs/16` §16 的未知项）**没接**。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
        self.force_update_cb();
    }

    /// 原版 `gui_input`（`0x4fedc0`，**推断**归属）：只处理 `InputEventKey` 的 `unicode`，
    /// 28 条死键/上标折回 ASCII，`'^'` 单独先判。
    ///
    /// 判定本体是 [`finput::intent_for_key`]；这里只负责"从引擎事件里拿 `unicode`"
    /// 与"把结论落到 `LineEdit` 上"。
    fn gui_input(&mut self, event: Gd<InputEvent>) {
        self.gui_input_calls += 1;

        // `try_cast` 消费 `event`，所以先判类型再取字段。
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        if !key.is_pressed() {
            return;
        }
        let Some(ch) = char::from_u32(key.get_unicode()) else {
            // `char::from_u32` 失败 = 代理区码位（原版 `0x4fef2b`–`0x4fef3d` 直接返回）。
            return;
        };
        self.apply_key(ch);
    }
}

#[godot_api]
impl GraphwarFunctionInput {
    /// **机制 1**：自造的、带参数信号（去空格之后的表达式）。
    #[signal]
    fn expression_changed(expression: GString);

    /// **机制 1**：自造的、带参数信号（提交的文本，**原样**）。
    #[signal]
    fn function_submitted(text: GString);

    /// 原版 `on_text_change`（导出方法，`0x4c42ff` 那个名字）：文本变了 ⇒ 重新去空格。
    ///
    /// 参数是 `text_changed(new_text)` 的那一个 `String` —— `Callable::from_object_method`
    /// 会按注册的元数（1）传参，多一个少一个都是错。
    #[func]
    fn on_text_change(&mut self, new_text: GString) {
        self.text_change_calls += 1;
        self.recompute_expression(new_text.to_string());
    }

    /// 原版 `on_size_changed`（导出方法）：`size_changed` 没有参数。
    #[func]
    fn on_size_changed(&mut self) {
        self.size_changed_calls += 1;
    }

    /// 原版 `on_text_submitted`（导出方法，`0x427070`）：把文本**原样**交给 `"fire"` 回调。
    ///
    /// 原版这条路上**没有任何字符串变换**（`docs/16` §16：`0x4f4250` 只做
    /// `[rdi+0x68]` 取值 + `call *0x20`）⇒ 传 `" s in ( x ) "` 就收到带空格的原文。
    #[func]
    fn on_text_submitted(&mut self, new_text: GString) {
        self.submit_calls += 1;
        let raw = new_text.to_string();
        let (method, text) = finput::submit_intent(&raw);
        debug_assert_eq!(method, "fire");

        // 机制 3 原版形态：一个 dyn 回调。
        if let Some(callback) = self.fire_native.as_mut() {
            callback(text);
        }
        // 机制 3 替代方案：任意多个 Callable。
        let arg = text.to_variant();
        for callable in self.fire_callables.iter() {
            callable.call(&[arg.clone()]);
        }

        self.submit_emits += 1;
        let payload = new_text.clone();
        let err = self
            .base_mut()
            .emit_signal("function_submitted", &[payload.to_variant()]);
        if err != godot::global::Error::OK {
            self.emit_errors.push(format!("function_submitted: {err:?}"));
        }
    }

    /// 原版 `force_update_cb`（导出方法，`0x4f2200`）：读引擎文本 → **去空格** → 变了就发信号。
    ///
    /// 返回去空格之后的结果。原版这一步之后还"送路径计算 + 与 game_client 对账"
    /// （`docs/16` §16 的角色描述），那两件**没接**。
    #[func]
    fn force_update_cb(&mut self) -> GString {
        self.force_update_calls += 1;
        let text = self.base().get_text().to_string();
        self.recompute_expression(text);
        self.expression.clone()
    }

    /// 最近一次去空格之后的表达式。
    #[func]
    fn expression(&self) -> GString {
        self.expression.clone()
    }

    /// **28 条折叠的可测入口**：走与 `gui_input` **同一个** `apply_key`，返回处理之后的文本。
    ///
    /// 返回 `""` 表示"没接管"（原版的默认分支，交给基类 `LineEdit`）。
    #[func]
    fn simulate_key(&mut self, unicode: i64, caret_condition: bool) -> GString {
        self.caret_condition = caret_condition;
        let Some(ch) = char::from_u32(unicode.clamp(0, 0x10ffff) as u32) else {
            return GString::new();
        };
        if self.apply_key(ch) {
            self.base().get_text()
        } else {
            GString::new()
        }
    }

    /// **机制 3 的替代方案**：登记一个 `Callable`，返回现在有几个。
    #[func]
    fn subscribe(&mut self, cb: Callable) -> i64 {
        if cb.is_valid() {
            self.fire_callables.push(cb);
        }
        self.fire_callables.len() as i64
    }

    /// **机制 3 的原版形态**：设那个**单个** dyn 回调（原版 `[+0x68]` 就只有一个）。
    #[func]
    fn subscribe_native_probe(&mut self) -> bool {
        let capture = self.native_capture.clone();
        self.fire_native = Some(Box::new(move |text: &str| {
            capture.borrow_mut().push(text.to_owned());
        }));
        self.fire_native.is_some()
    }

    /// `_ready` 里真连上了几条信号（原版写的是 3 条，实测只有 2 条能连上）。
    #[func]
    fn wired_count(&self) -> i64 {
        self.wired
    }

    /// `_ready` 里连不上的信号（`信号名: 错误码`，`|` 分隔）。
    ///
    /// 非空就是"原版那条 connect 在本机 Godot 上连不上"的证据。
    #[func]
    fn connect_errors(&self) -> GString {
        GString::from(join_text(&self.connect_errors).as_str())
    }

    /// `gui_input` 被调到几次（`--script` 模式下大概率是 0，见文件头 §3 第 1 条）。
    #[func]
    fn gui_input_call_count(&self) -> i64 {
        self.gui_input_calls
    }

    /// `on_text_change` 被调到几次。
    #[func]
    fn text_change_count(&self) -> i64 {
        self.text_change_calls
    }

    /// `on_text_submitted` 被调到几次。
    #[func]
    fn submit_count(&self) -> i64 {
        self.submit_calls
    }

    /// `on_size_changed` 被调到几次。
    #[func]
    fn size_changed_count(&self) -> i64 {
        self.size_changed_calls
    }

    /// `_ready` 被调到几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `_process` 被调到几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `force_update_cb` 被调到几次。
    #[func]
    fn force_update_count(&self) -> i64 {
        self.force_update_calls
    }

    /// 往 `LineEdit` 里插过几次字（`FN_INSERT_TEXT` 的落点）。
    #[func]
    fn insert_count(&self) -> i64 {
        self.insert_count
    }

    /// `expression_changed` 发了几次。
    #[func]
    fn expression_emit_count(&self) -> i64 {
        self.expression_emits
    }

    /// `function_submitted` 发了几次。
    #[func]
    fn submit_emit_count(&self) -> i64 {
        self.submit_emits
    }

    /// 那个单 dyn 回调收到过的文本（`|` 分隔）。
    #[func]
    fn fire_captured(&self) -> GString {
        GString::from(join_text(&self.native_capture.borrow()).as_str())
    }

    /// `Callable` 收到过的文本（`|` 分隔）。
    #[func]
    fn callable_captured(&self) -> GString {
        GString::from(join_text(&self.callable_capture.borrow()).as_str())
    }

    /// emit 失败的记录（`|` 分隔；空 = 一次都没失败）。
    #[func]
    fn emit_errors(&self) -> GString {
        GString::from(join_text(&self.emit_errors).as_str())
    }

    // -----------------------------------------------------------------------
    // 内部
    // -----------------------------------------------------------------------

    /// 去空格并把结果写进 `self.expression`；**变了才**发信号（原版 `force_update_cb` 的语义）。
    fn recompute_expression(&mut self, raw: String) {
        let stripped = finput::strip_spaces(&raw);
        if self.expression.to_string() == stripped {
            return;
        }
        self.expression = GString::from(stripped.as_str());
        self.expression_emits += 1;
        // 机制 1：这条走**裸** `emit_signal`（与 GraphwarButton 的类型安全生成器对照）。
        let payload = self.expression.clone();
        let err = self
            .base_mut()
            .emit_signal("expression_changed", &[payload.to_variant()]);
        if err != godot::global::Error::OK {
            self.emit_errors.push(format!("expression_changed: {err:?}"));
        }
    }

    /// 一次按键的落点：`InsertAndRefresh` ⇒ 往光标处插字 + `force_update_cb`。
    ///
    /// 返回 `true` = 本类接管了（原版插完字就返回，不再交给基类）。
    fn apply_key(&mut self, ch: char) -> bool {
        match finput::intent_for_key(ch, self.caret_condition) {
            finput::InputIntent::Delegate => false,
            finput::InputIntent::InsertAndRefresh(text) => {
                // 原版是 `format!("{}", ch)` 成 `String` 再调 `FN_INSERT_TEXT`（`0x4f4320`）。
                let payload = GString::from(text.as_str());
                self.base_mut().insert_text_at_caret(&payload);
                self.insert_count += 1;
                // 插入之后 `call("force_update_cb")`（`REFRESH_METHOD`）。
                self.force_update_cb();
                true
            }
        }
    }
}

/// 把一串字符串接成 `a|b|c`。
///
/// **返回 `String` 而不是 `GString`**：`GString::from` 要调引擎的构造函数，没有 Godot 进程
/// 就 panic，所以纯 Rust 的那一半留在 `String` 上，到 `#[func]` 边界才转。
fn join_text<S: AsRef<str>>(items: &[S]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push('|');
        }
        out.push_str(item.as_ref());
    }
    out
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `ready` 接的是原版那三对，而且**信号名与处理函数名不同名**（与 GraphwarButton 对照）。
    #[test]
    fn ready_wiring_pairs_are_the_original_three() {
        assert_eq!(
            finput::SIGNAL_WIRING,
            [
                ("size_changed", "on_size_changed"),
                ("text_changed", "on_text_change"),
                ("text_submitted", "on_text_submitted"),
            ]
        );
        for (signal, handler) in finput::SIGNAL_WIRING {
            assert_ne!(signal, handler, "这一组本来就是两个不同的名字");
            assert_eq!(finput::handler_for_signal(signal), Some(handler));
            assert_eq!(finput::signal_for_handler(handler), Some(signal));
        }
    }

    /// `LOG_LIMIT` 与"去空格是唯一变换"这两件事在纯逻辑里都成立（本类不另立规则）。
    #[test]
    fn the_only_preprocessing_is_space_stripping() {
        assert_eq!(finput::strip_spaces("sin ( x )"), "sin(x)");
        // 不转 π、不小写、不动其它空白 —— 那都在解析器里。
        assert_eq!(finput::strip_spaces("PI + π"), "PI+π");
        assert!(LOG_LIMIT > 0);
    }

    /// `'^'` 走的是那个未知条件参数：`true` 接管、`false` 交回基类。
    #[test]
    fn caret_key_depends_on_the_unknown_condition_parameter() {
        assert_eq!(
            finput::intent_for_key('^', true),
            finput::InputIntent::InsertAndRefresh("^".into())
        );
        assert_eq!(finput::intent_for_key('^', false), finput::InputIntent::Delegate);
    }

    /// 折叠表的两端：`â` 折成 `a`、ASCII 一律不接管。
    #[test]
    fn composed_keys_fold_and_ascii_is_delegated() {
        assert_eq!(
            finput::intent_for_key('\u{00e2}', false),
            finput::InputIntent::InsertAndRefresh("a".into())
        );
        assert_eq!(finput::intent_for_key('x', true), finput::InputIntent::Delegate);
        assert_eq!(finput::fold_composed('π'), None, "π 的规范化在解析器");
    }

    /// 提交路径不改写文本（本类的 `on_text_submitted` 就是照这一条写的）。
    #[test]
    fn submit_intent_passes_text_verbatim() {
        assert_eq!(finput::submit_intent(" s in ( x ) "), ("fire", " s in ( x ) "));
    }
}
