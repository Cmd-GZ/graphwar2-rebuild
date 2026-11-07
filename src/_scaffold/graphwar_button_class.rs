//! **不是原版的东西**：把复原的 [`crate::graphwar_button`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarButton` 是 **`Button` 的子类**（`impl IButton`，见
//! [`graphwar_button::TRAIT_IMPLS`]）：长得像虚拟键盘上的一个键，点一下把自己的
//! `input_text` 发给**一张外部注册的回调表**，按住时把 `long_press_input_text` 发一次。
//! `src/graphwar_button.rs` 已经把这 6 个方法的**状态机**复原出来了，但它按约定**不碰引擎**
//! （一行 Godot 调用都没有）。本文件是"让那个状态机真的挂到节点上"的那层薄壳：
//!
//! ```text
//! src/graphwar_button.rs            纯逻辑：SIGNAL_LINKS / ButtonState / Stamp / Effect
//! src/_scaffold/graphwar_button_class.rs   ← 本文件：connect 信号、持有回调表、发信号
//! ```
//!
//! 状态机本体（`button_down` / `button_up` / `process` / `pressed` 的判定顺序）**只有一份**，
//! 仍然在 `src/graphwar_button.rs` 里；本文件不复制任何判定。
//!
//! # 本轮要验的三种机制（结论见 `docs/16-client.md` §35）
//!
//! | # | 机制 | 原版形态 | 本文件怎么用 |
//! |---|---|---|---|
//! | 1 | **自造 `#[signal]`** | 无（原版 6 个方法里没有 `emit_signal`） | `#[signal] long_press_finished()` / `input_selected(text)`，用**类型安全生成器** `self.signals().…().emit()` |
//! | 2 | **连引擎原生信号** | `ready` 里 3 条 `connect`（[`graphwar_button::SIGNAL_LINKS`]） | `self.base_mut().connect("pressed", callable(self,"pressed"))` 等 3 条 |
//! | 3 | **外部注册的闭包表** | `Vec<Box<dyn FnMut(&str)>>`（`+0x38/+0x40/+0x48`） | 字段就写这个类型（原样）；另存一份 `Vec<Callable>` 给 GDScript |
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 / trait | `Button` + `IButton`（`TRAIT_IMPLS`） | 同 | **观测** |
//! | `ready` 的 3 条连接 | `pressed`/`button_up`/`button_down` → 同名方法 | 同（信号名与处理函数名是同一个 `&str`，见 [`graphwar_button::SIGNAL_LINKS`]） | **观测** |
//! | `ready` 的按名找节点 | `get_node_by_name_rec` 找 `"AnimationRect"` 并 cast 成 `GraphwarLongPressAnimation` | 用 `find_child` 找同名节点，**只记 bool**；那个类没复原，所以**不 cast、不持句柄** | 名字**观测**；行为**未做** |
//! | 订阅者表 | `Vec<Box<dyn FnMut(&str)>>`，由谁 `push` **未知** | 同一个类型；`subscribe_native_probe()` 由 Rust 侧塞一个探针闭包，`subscribe(Callable)` 给 GDScript | 类型**观测**；注册点是**我方选择** |
//! | 长按阈值 500 ms | `+0x50/+0x58` = `Duration{0, 0x1dcd6500}`，6 个方法里**没有读者** | 照抄进 `ButtonState::hold`，同样**不读**（`process` 只看"还按着"） | **观测** |
//! | 时间戳 | `Instant::now()`（`+0x60` secs / `+0x68` nanos），哨兵 = nanos 1e9 | 本实例构造时锚一个 `std::time::Instant`，之后用 `elapsed()` 造 [`Stamp`] | 语义**推断**（原版是绝对时刻，我们只要"与哨兵可比"） |
//! | 动画节点上的两个 `Instant` | `button_down`/`button_up` 会写 `[lpa+0x18..+0x20]` | **没写**（那个类没有节点实现） | **未做** |
//! | `queue_redraw` | `base_mut() → queue_redraw()` | 同（`Effect::QueueRedraw` 的消费者） | **观测** |
//! | 自造信号 | **原版没有**（`pressed`/`button_up`/`button_down` 都是 `BaseButton` 的） | 本文件加了 2 个，供宿主观察 | 我方**新增** |
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/signal_classes_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarButton : Button
//!   @export input_text : String              # 原版 +0x88
//!   @export long_press_input_text : String   # 原版 +0x90
//!
//!   signal long_press_finished()             # 自造：长按文本派发完了
//!   signal input_selected(text: String)      # 自造：点一下派发的文本
//!
//!   pressed() / button_down() / button_up()  # 原版同名方法（被 ready 里的 connect 指着）
//!   subscribe(cb: Callable) -> int           # 替代方案：把 GDScript 的 Callable 塞进表里
//!   subscribe_native_probe() -> int          # 原版形态：Rust 侧 push 一个 Box<dyn FnMut(&str)>
//!   unsubscribe_all() -> void
//!   pressed_call_count() / button_down_call_count() / button_up_call_count() -> int
//!   ready_call_count() / process_call_count() -> int
//!   long_press_emit_count() / input_selected_emit_count() -> int
//!   subscriber_count() -> int                # = Vec<Box<dyn FnMut>>.len()
//!   callable_count() -> int
//!   native_captured() / callable_captured() / dispatch_log() -> String
//!   press_stamp_is_pressed() -> bool
//!   animation_linked() -> bool
//!   reentrancy_probe() -> String             # 见 §"坑 3"
//! ```
//!
//! # 写这类"带信号"的类必须知道的五条（本轮实测）
//!
//! 1. **`Gd<Node>` 不保活**（`entity_drawer_class.rs` §坑 1 的同一件事）。本文件**故意不**
//!    缓存 `AnimationRect` 的句柄：我们拿它没办法（那个类没实现），缓存一个随时可能被
//!    `free()` 的野句柄只会换来 `upcast_ref: access to instance … after it has been freed`
//!    + `error code 1337` + 进程卡死。只存 `bool`。
//! 2. **`#[signal]` 的生成器要 `&mut self`**（`WithUserSignals::signals(&mut self)`）⇒
//!    发信号时**不能再借 `self` 的字段**。所有要发的值必须先算成局部变量。
//! 3. **闭包表不能捕获 `Gd<Self>`**：在 `#[func](&mut self)` 里调 `self.to_gd().bind_mut()`
//!    会撞"已经可变借用"。本文件用 `Rc<RefCell<Vec<String>>>` 这一层间接来记录，
//!    [`GraphwarButton::reentrancy_probe`] 把那条 panic **抓成返回值**（不炸进程）。
//! 4. **信号名与处理函数名可以同名**（原版 `pressed` 既是被连的信号、也是方法名），
//!    但**自造信号名不能与 `BaseButton` 的已有信号同名**（`pressed`/`button_up`/
//!    `button_down`/`toggled` 都是基类的），所以本文件自造的两个信号叫
//!    `long_press_finished` / `input_selected`。
//! 5. **`_ready` 里才连信号** ⇒ 树外 `emit_signal("pressed")` 不会被我们收到
//!    （连接还没建立）。验收脚本因此都先 `root.add_child` 再发信号。

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use godot::classes::{Button, IButton};
use godot::obj::WithUserSignals;
use godot::prelude::*;

use crate::graphwar_button::{self, ButtonState, Stamp};

/// 一次派发之后，`dispatch_log` 最多留多少条（防止长按连发把内存吃光）。
const DISPATCH_LOG_LIMIT: usize = 32;

/// `GraphwarButton`：虚拟键盘上的一个键。
///
/// 状态机的**唯一**持有者是 [`graphwar_button::ButtonState`]；本结构只往里塞
/// `Stamp`、把 `Effect` 落到引擎调用上、以及在派发时驱动两张回调表。
#[derive(GodotClass)]
#[class(base = Button)]
pub struct GraphwarButton {
    base: Base<Button>,

    /// 原版 `[self+0x88]`（`pressed` 里 `add r?,0x88` 那个字段）。
    ///
    /// ⚠️ `#[export]` 是**直接读写字段**，没有 setter 钩子 ⇒ 每次派发之前
    /// [`GraphwarButton::sync_from_properties`] 把它抄进状态机。
    #[export]
    input_text: GString,

    /// 原版 `[self+0x90]`（`process` 里 `lea rbx,[rdi+0x90]` 那个字段）。
    #[export]
    long_press_input_text: GString,

    /// 原版的状态机（6 个方法的全部判定）。
    state: ButtonState,

    /// **机制 3 的原版形态**：`Vec<Box<dyn FnMut(&str)>>`（`+0x38/+0x40/+0x48`）。
    ///
    /// 这是原版逐字段复原出来的类型，本文件**原样**用它。见
    /// [`GraphwarButton::subscribe_native_probe`]。
    subscribers: Vec<Box<dyn FnMut(&str)>>,

    /// 探针闭包往这里写：`Rc<RefCell<_>>` 是"闭包不能借 `self`"的绕法（§坑 3）。
    native_capture: Rc<RefCell<Vec<String>>>,

    /// **机制 3 的替代方案**：GDScript 能给过来的只有 `Callable`
    /// （`Variant` 的类型表里没有"Rust 闭包"这一项）。
    callables: Vec<Callable>,

    /// GDScript 侧 Callable 的到货记录。
    callable_capture: Rc<RefCell<Vec<String>>>,

    /// `ready` 里按名找到 `"AnimationRect"` 了吗（原版 `[self+0x18]` 的判别位）。
    animation_linked: bool,

    /// 时间戳的锚点：原版用绝对 `Instant::now()`，我们只要"与哨兵可比"。
    epoch: Instant,

    /// 派发过的文本（按顺序，最多 [`DISPATCH_LOG_LIMIT`] 条）。
    dispatch_log: Vec<String>,

    ready_calls: i64,
    process_calls: i64,
    pressed_calls: i64,
    button_down_calls: i64,
    button_up_calls: i64,
    long_press_emits: i64,
    input_selected_emits: i64,
}

#[godot_api]
impl IButton for GraphwarButton {
    fn init(base: Base<Button>) -> Self {
        Self {
            base,
            input_text: GString::new(),
            long_press_input_text: GString::new(),
            state: ButtonState::new(),
            subscribers: Vec::new(),
            native_capture: Rc::new(RefCell::new(Vec::new())),
            callables: Vec::new(),
            callable_capture: Rc::new(RefCell::new(Vec::new())),
            animation_linked: false,
            epoch: Instant::now(),
            dispatch_log: Vec::new(),
            ready_calls: 0,
            process_calls: 0,
            pressed_calls: 0,
            button_down_calls: 0,
            button_up_calls: 0,
            long_press_emits: 0,
            input_selected_emits: 0,
        }
    }

    /// `_ready`：**机制 2** —— 连 3 条 `BaseButton` 原生信号（顺序即
    /// [`graphwar_button::SIGNAL_LINKS`] 的指令顺序），再按名找 `"AnimationRect"`。
    ///
    /// 原版两件事的证据都在 `0x4e3f20`：三个 `lea "pressed"/"button_up"/"button_down"`
    /// （`0x4e3f4e`/`0x4e3fc7`/`0x4e4040`）各配一次 `connect`，然后
    /// `lea "AnimationRect"`（`0x4e410d`）走 `get_node_by_name_rec` + cast。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // `Callable::from_object_method(self, name)` 的 gdext 写法：先拿 `Gd<Self>`。
        let myself = self.to_gd();
        for link in graphwar_button::SIGNAL_LINKS {
            let callable = myself.callable(link.name);
            // 同一个 `&str` 既是信号名、也是处理函数名 —— 这是原版的形态（§5）。
            let err = self.base_mut().connect(link.name, &callable);
            if err != godot::global::Error::OK {
                godot_warn!("GraphwarButton::ready 连接 {} 失败: {err:?}", link.name);
            }
        }

        // 按名找动画节点。**递归**查找 = 原版 `get_node_by_name_rec` 的语义；
        // 只记 bool，不持句柄（§坑 1）。
        // ⚠️ `find_child` 的 `owned` 默认是 **true**（Godot 4.6 的默认参数），而代码里
        // `add_child` 建出来的节点 `owner` 是 null ⇒ 必须显式 `owned(false)`，否则永远找不到。
        let found = self
            .base()
            .find_child_ex(graphwar_button::NODE_ANIMATION_RECT)
            .recursive(true)
            .owned(false)
            .done();
        self.animation_linked = found.is_some();
        // 状态机里那个 `[self+0x18]` 判别位（`cmp dword [r+0x18],1`）跟着走：
        // 没接动画节点 ⇒ `button_down`/`button_up` 不产生动画效果（原版同一分支）。
        self.state.animation_linked = self.animation_linked;
    }

    /// `_process`：原版 `0x4e42a0` 的**长按一次性派发**。
    ///
    /// 判定顺序逐条照抄 [`GraphwarButton::process`] 的调用方：
    /// 文本空 → 已释放 → 置哨兵 → 无订阅者（前三步都在 `ButtonState::process` 里）。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
        // `#[export]` 字段是**直接读写**的、没有 setter 钩子（实测），而状态机读的是自己那份
        // 拷贝 ⇒ 每帧读之前必须同步一次，否则 GDScript 改了 `long_press_input_text` 也不生效。
        self.sync_from_properties();

        // 先把借用结束掉再动 `self`（`ButtonState::process` 返回 `&str`，借的是 `self.state`）。
        let dispatched: Option<String> = self
            .state
            .process()
            .map(|text| text.to_owned());
        let Some(text) = dispatched else {
            return;
        };

        self.dispatch(&text);
        self.emit_long_press_finished();
    }
}

#[godot_api]
impl GraphwarButton {
    /// **机制 1**：自造的、无参信号。
    ///
    /// 原版**没有**这条信号（6 个方法里一个 `emit_signal` 都没有）—— 它存在的唯一理由是
    /// 让宿主（GDScript / 别的节点）能观察到"长按派发完了"，而不必去订阅那张闭包表。
    #[signal]
    fn long_press_finished();

    /// **机制 1**：自造的、带参数（`String`）的信号。
    #[signal]
    fn input_selected(text: GString);

    /// 原版 `GraphwarButton::pressed`（`0x4ef560`）：把 `input_text` 交给每个订阅者。
    ///
    /// 原版**不改状态**（只读 `+0x88`），这里也一样。它是 `#[func]` 而不是
    /// `IButton::pressed` 虚方法 —— 原版那条 `connect("pressed", …)` 指的就是一个能按名字
    /// 找到的方法（`docs/16` §19 的 9 个名字分派入口里有 `GraphwarButton pressed`）。
    ///
    /// ⚠️ 注意它与 `BaseButton` 的**虚方法** `pressed`（Godot 侧 `_pressed`）是两回事：
    /// 本类**没有**实现那个虚方法，只连信号。
    #[func]
    fn pressed(&mut self) {
        self.pressed_calls += 1;
        self.sync_from_properties();
        let text = self.state.pressed().to_owned();
        self.dispatch(&text);
        self.emit_input_selected(&text);
    }

    /// 原版 `GraphwarButton::button_down`（`0x4ef390`）：**先**记时间，**再**看有没有动画节点。
    ///
    /// 原版真的调了两次 `Instant::now()`（`0x4ef39d` 写自己、`0x4ef3e2` 写动画节点）
    /// ⇒ 这里也取两次 [`Stamp`]。
    #[func]
    fn button_down(&mut self) {
        self.button_down_calls += 1;
        let now = self.stamp();
        let again = self.stamp();
        let effects = self.state.button_down(now, again);
        self.apply_effects(&effects);
    }

    /// 原版 `GraphwarButton::button_up`（`0x4ef6a0`）：把两处 nanos 写成哨兵，再请求重绘。
    #[func]
    fn button_up(&mut self) {
        self.button_up_calls += 1;
        let effects = self.state.button_up();
        self.apply_effects(&effects);
    }

    /// **机制 3 的替代方案**：把 GDScript 的 `Callable` 注册进来，返回新的订阅者个数。
    ///
    /// 原版的表装的是 `Box<dyn FnMut(&str)>`；`Variant` 的类型表里**没有**这一项
    /// （见 `docs/16-client.md` §35.4），所以 GDScript 只能给 `Callable`。
    #[func]
    fn subscribe(&mut self, cb: Callable) -> i64 {
        if cb.is_valid() {
            self.callables.push(cb);
        }
        self.state.subscribers = self.subscribers.len() + self.callables.len();
        self.state.subscribers as i64
    }

    /// **机制 3 的原版形态**：在 Rust 侧往 `Vec<Box<dyn FnMut(&str)>>` 里 push 一个探针闭包。
    ///
    /// 闭包捕获的是 [`GraphwarButton::native_capture`]（`Rc<RefCell<Vec<String>>>`），
    /// **不是** `self` —— 见文件头 §坑 3。
    #[func]
    fn subscribe_native_probe(&mut self) -> i64 {
        let capture = self.native_capture.clone();
        self.subscribers.push(Box::new(move |text: &str| {
            capture.borrow_mut().push(text.to_owned());
        }));
        self.state.subscribers = self.subscribers.len() + self.callables.len();
        self.state.subscribers as i64
    }

    /// 清空两张表（`Vec<Box<dyn FnMut>>` 与 `Vec<Callable>`）。
    #[func]
    fn unsubscribe_all(&mut self) {
        self.subscribers.clear();
        self.callables.clear();
        self.state.subscribers = 0;
    }

    /// `pressed` 被调用过几次。
    #[func]
    fn pressed_call_count(&self) -> i64 {
        self.pressed_calls
    }

    /// `button_down` 被调用过几次。
    #[func]
    fn button_down_call_count(&self) -> i64 {
        self.button_down_calls
    }

    /// `button_up` 被调用过几次。
    #[func]
    fn button_up_call_count(&self) -> i64 {
        self.button_up_calls
    }

    /// `_ready` 被调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `_process` 被调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `long_press_finished` 发了几次（**我们发**的次数，不是别人收到的次数）。
    #[func]
    fn long_press_emit_count(&self) -> i64 {
        self.long_press_emits
    }

    /// `input_selected` 发了几次。
    #[func]
    fn input_selected_emit_count(&self) -> i64 {
        self.input_selected_emits
    }

    /// `Vec<Box<dyn FnMut(&str)>>` 里有几个（原版 `[self+0x48]` 那个长度）。
    #[func]
    fn subscriber_count(&self) -> i64 {
        self.subscribers.len() as i64
    }

    /// `Vec<Callable>` 里有几个。
    #[func]
    fn callable_count(&self) -> i64 {
        self.callables.len() as i64
    }

    /// 探针闭包收到过的文本（`|` 分隔）—— **非空就证明 `Box<dyn FnMut(&str)>` 真的被调到了**。
    #[func]
    fn native_captured(&self) -> GString {
        GString::from(join_text(&self.native_capture.borrow()).as_str())
    }

    /// GDScript 的 `Callable` 收到过的文本（`|` 分隔）。
    #[func]
    fn callable_captured(&self) -> GString {
        GString::from(join_text(&self.callable_capture.borrow()).as_str())
    }

    /// 派发流水（`|` 分隔，最多 [`DISPATCH_LOG_LIMIT`] 条）。
    #[func]
    fn dispatch_log(&self) -> GString {
        GString::from(join_text(&self.dispatch_log).as_str())
    }

    /// 状态机现在认为"按着"吗（`cmp dword [r+0x68],0x3b9aca00` 的否定）。
    #[func]
    fn press_stamp_is_pressed(&self) -> bool {
        self.state.press.is_pressed()
    }

    /// `ready` 按名找到 `"AnimationRect"` 了吗。
    #[func]
    fn animation_linked(&self) -> bool {
        self.animation_linked
    }

    /// **把"闭包捕获 `Gd<Self>` 会怎样"抓成返回值**（§坑 3）。
    ///
    /// 这里**故意**在 `#[func](&mut self)` 里对同一个实例再要一次 `bind_mut()`；
    /// gdext 的 balanced safeguard 会 panic。`catch_unwind` 把它接住，于是
    /// 验收脚本能拿到一条字符串证据，而不是让整个 headless 进程卡死。
    ///
    /// 返回 `"no panic: …"` 或 `"panic: …"`。
    #[func]
    fn reentrancy_probe(&mut self) -> GString {
        let mut myself = self.to_gd();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _second = myself.bind_mut();
        }));
        match caught {
            Ok(()) => GString::from("no panic: second bind_mut() succeeded"),
            Err(payload) => {
                let text = panic_text(&payload);
                GString::from(format!("panic: {text}").as_str())
            }
        }
    }

    // -----------------------------------------------------------------------
    // 内部：状态机 ↔ 引擎
    // -----------------------------------------------------------------------

    /// 从两个导出属性把 `input_text` / `long_press_input_text` 抄进状态机。
    ///
    /// `#[export]` 字段是直接访问的（没有 setter 钩子），所以每次派发前同步一次。
    fn sync_from_properties(&mut self) {
        self.state.input_text = self.input_text.to_string();
        self.state.long_press_input_text = self.long_press_input_text.to_string();
        self.state.subscribers = self.subscribers.len() + self.callables.len();
    }

    /// `Duration` 的当前值（原版 `Instant::now()` 的两个半区）。
    fn stamp(&self) -> Stamp {
        let elapsed = self.epoch.elapsed();
        Stamp::now(elapsed.as_secs(), elapsed.subsec_nanos())
    }

    /// 把 `ButtonState` 算出来的 [`graphwar_button::Effect`] 落到引擎调用上。
    ///
    /// 状态改动**已经在** `ButtonState` 里做完了；这里只剩"请求重绘"这一件真事
    /// （`RecordPress`/`StartAnimation`/`StopAnimation` 记的是被写进字段的时刻，
    /// 而动画节点那个类还没有节点实现，见文件头对照表最后两行）。
    fn apply_effects(&mut self, effects: &[graphwar_button::Effect]) {
        for effect in effects {
            if matches!(effect, graphwar_button::Effect::QueueRedraw) {
                self.base_mut().queue_redraw();
            }
        }
    }

    /// 把 `text` 交给**两张**回调表（原版只有第一张；第二张是给 GDScript 的替代方案）。
    fn dispatch(&mut self, text: &str) {
        if self.dispatch_log.len() >= DISPATCH_LOG_LIMIT {
            self.dispatch_log.remove(0);
        }
        self.dispatch_log.push(text.to_owned());

        // 机制 3 原版形态：`for f in self.subscribers { f(text) }`
        for callback in self.subscribers.iter_mut() {
            callback(text);
        }

        // 机制 3 替代方案：`Callable::call(&[Variant])`
        let arg = text.to_variant();
        for callable in self.callables.iter() {
            callable.call(&[arg.clone()]);
        }
    }

    /// **机制 1**：用 gdext 的类型安全生成器发无参信号。
    fn emit_long_press_finished(&mut self) {
        self.long_press_emits += 1;
        self.signals().long_press_finished().emit();
    }

    /// **机制 1**：发带参数的信号（`GString` 参数在 GDScript 侧就是 `String`）。
    fn emit_input_selected(&mut self, text: &str) {
        self.input_selected_emits += 1;
        let payload = GString::from(text);
        self.signals().input_selected().emit(&payload);
    }
}

/// 把一串字符串接成 `a|b|c`（空表 → `""`）。
///
/// **返回 `String` 而不是 `GString`**：`GString::from` 要调引擎的构造函数，没有 Godot 进程
/// 就 panic（`single_threaded.rs:170`），所以纯 Rust 的那一半必须留在 `String` 上，
/// 到 `#[func]` 边界才 `GString::from`。本文件初版就在这里栽过一次（`cargo test` 红）。
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

/// `catch_unwind` 的 payload → 可读文本（`panic!` 的两种载荷都照顾到）。
fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉"这个类用的是哪份常量/哪份状态机"）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/signal_classes_check.gd`（headless，真引擎）。
// 这里只保证：① 类里没有第二份状态机；② 三个原生信号的名字就是原版那三个。

#[cfg(test)]
mod tests {
    use super::*;

    /// `ready` 连的必须是原版那 3 条，且信号名 = 处理函数名（同一个 `&str`）。
    #[test]
    fn ready_connects_exactly_the_three_original_signals() {
        let names: Vec<&str> = graphwar_button::SIGNAL_LINKS.iter().map(|l| l.name).collect();
        assert_eq!(names, vec!["pressed", "button_up", "button_down"]);
        for link in graphwar_button::SIGNAL_LINKS {
            // 原版是"信号名与处理函数名同一地址"⇒ 名字相同。
            assert_eq!(link.name.len() as u8, link.len, "{}", link.name);
        }
    }

    /// 自造信号**不能**用基类已有的信号名（`BaseButton` 有 `pressed`/`button_up`/
    /// `button_down`/`toggled`）⇒ 本条同时锁住"我们加了哪两个"。
    #[test]
    fn custom_signals_do_not_shadow_base_class_signals() {
        let base_signals = ["pressed", "button_up", "button_down", "toggled"];
        for name in ["long_press_finished", "input_selected"] {
            assert!(
                !base_signals.contains(&name),
                "{name} 与 BaseButton 的信号同名，Godot 侧会打架"
            );
        }
    }

    /// 初始状态 = `init`（`0x4e3e00`）写出来的那 7 个值。
    #[test]
    fn initial_state_matches_the_original_init() {
        let state = ButtonState::new();
        assert_eq!(state.press, Stamp::NONE);
        assert_eq!(state.press.nanos, graphwar_button::NANOS_PER_SEC);
        assert_eq!(state.hold.nanos, graphwar_button::LONG_PRESS_HOLD_NANOS);
        assert!(!state.animation_linked);
        assert_eq!(state.subscribers, 0);
        assert!(!state.press.is_pressed());
    }

    /// **没有第二份状态机**：一次"按下 → 派发 → 抬起"的判定全来自 `ButtonState`。
    ///
    /// 这也顺便钉住了"长按只发一次"（`process` 第二次返回 `None`）。
    #[test]
    fn long_press_dispatch_comes_from_the_pure_state_machine() {
        let mut state = ButtonState::new();
        state.long_press_input_text = "S".to_owned();
        // 原版 `process` 的第 4 步是"无订阅者就返回"（`+0x48` 当长度）⇒ 先得有 1 个。
        state.subscribers = 1;

        // 还没按下 ⇒ 不发。
        assert_eq!(state.process(), None);

        let now = Stamp::now(7, 123);
        state.button_down(now, now);
        assert!(state.press.is_pressed());
        assert_eq!(state.process(), Some("S"));
        // 一次性：第二次已经是哨兵。
        assert_eq!(state.process(), None);
        assert!(!state.press.is_pressed());
    }

    /// `Stamp` 的哨兵语义：`elapsed()` 的 nanos 永远 < 1e9，所以「按着」判定永假不了。
    #[test]
    fn a_fresh_elapsed_stamp_is_always_pressed() {
        for nanos in [0u32, 1, 999_999_999] {
            assert!(Stamp::now(0, nanos).is_pressed());
        }
        assert!(!Stamp::NONE.released().is_pressed());
    }

    /// `join_text` 的分隔符与空表行为（验收脚本按 `|` 切串）。
    #[test]
    fn join_text_uses_pipe_and_handles_empty() {
        assert_eq!(join_text::<&str>(&[]), "");
        assert_eq!(join_text(&["a"]), "a");
        assert_eq!(join_text(&["a", "b"]), "a|b");
    }
}
