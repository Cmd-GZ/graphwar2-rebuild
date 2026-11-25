//! **不是原版的东西**：把 [`super::replay_meta::ReplayMeta`] 挂成 GDExtension 类。
//!
//! 类名 `GwReplayMeta`，只给回放列表（`client/src/screens/replay_lobby.gd`）逐文件查
//! "这一行显示什么"。**这里没有第二份解析**：字段抽取/时间戳格式化全在
//! `replay_meta.rs`（纯逻辑，`cargo test` 覆盖），本文件只做
//! `Rust 值 ↔ Godot Variant` 的搬运与 `path` 的读盘。
//!
//! # 与 GDScript 的接口
//!
//! ```gdscript
//! var m := GwReplayMeta.new()
//! m.read(ProjectSettings.globalize_path("user://replays/1793897298"))  # false = 读不到
//! m.game_name            # "Level 0"
//! m.mode_icons()         # ["axis_option_3","fx_option","turns_sequential","turn_inf"]
//! m.format_utc(1793897298)  # "2026-11-05 16:48:18"  ← 静态方法
//! ```
//!
//! 证据等级与"为什么读回放内容"见 `replay_meta.rs` 的文件头（`docs/06` §56）。

use godot::classes::RefCounted;
use godot::prelude::*;

use super::replay_meta::{format_utc, parse_head, timestamp_from_name, ReplayMeta};

#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwReplayMeta {
    base: Base<RefCounted>,
    meta: ReplayMeta,
}

#[godot_api]
impl IRefCounted for GwReplayMeta {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, meta: ReplayMeta::default() }
    }
}

#[godot_api]
impl GwReplayMeta {
    // ---------------------------------------------------------------- 读

    /// 读一个回放文件并抽元数据（真实路径；`user://` 请先用
    /// `ProjectSettings.globalize_path()` 换掉）。返回 `false` 时原因在 [`Self::error`]。
    #[func]
    fn read(&mut self, path: GString) -> bool {
        self.meta = ReplayMeta::from_path(std::path::Path::new(&path.to_string()));
        self.meta.ok
    }

    /// 直接吃一段**明文** JSONL（自检用；不经过 gzip）。
    #[func]
    fn read_text(&mut self, text: GString) -> bool {
        self.meta = parse_head(&text.to_string());
        self.meta.ok
    }

    // ---------------------------------------------------------------- 字段

    #[func]
    fn ok(&self) -> bool {
        self.meta.ok
    }

    #[func]
    fn error(&self) -> GString {
        GString::from(self.meta.error.as_str())
    }

    /// 关卡名（原版第 3 段）：回放内 `GameNameInfo.game_name` 的**最后一个**值。
    #[func]
    fn game_name(&self) -> GString {
        GString::from(self.meta.game_name.as_str())
    }

    #[func]
    fn axis_mode(&self) -> GString {
        GString::from(self.meta.axis_mode.as_str())
    }

    #[func]
    fn function_mode(&self) -> GString {
        GString::from(self.meta.function_mode.as_str())
    }

    #[func]
    fn turn_mode(&self) -> GString {
        GString::from(self.meta.turn_mode.as_str())
    }

    #[func]
    fn time_mode(&self) -> GString {
        GString::from(self.meta.time_mode.as_str())
    }

    /// 四个模式图标名（`GwIcons` 的键），顺序 = `replay_entry.tscn` 的
    /// `AxisModeButton` / `FunctionModeButton` / `TurnModeButton` / `TimeModeButton`。
    /// 某一格是空串 ⇒ 回放里没有这条事件，界面**不画**（不猜默认档）。
    #[func]
    fn mode_icons(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in self.meta.mode_icons() {
            out.push(name);
        }
        out
    }

    /// 全部字段一起取（自检/诊断用；键名与 `ReplayMeta` 字段一一对应）。
    #[func]
    fn to_dict(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let error = GString::from(self.meta.error.as_str());
        let game_name = GString::from(self.meta.game_name.as_str());
        let axis_mode = GString::from(self.meta.axis_mode.as_str());
        let function_mode = GString::from(self.meta.function_mode.as_str());
        let turn_mode = GString::from(self.meta.turn_mode.as_str());
        let time_mode = GString::from(self.meta.time_mode.as_str());
        d.set("ok", self.meta.ok);
        d.set("error", &error);
        d.set("game_name", &game_name);
        d.set("axis_mode", &axis_mode);
        d.set("function_mode", &function_mode);
        d.set("turn_mode", &turn_mode);
        d.set("time_mode", &time_mode);
        d.set("scanned", self.meta.scanned as i64);
        d.set("plain_len", self.meta.plain_len as i64);
        d.set("gzipped", self.meta.gzipped);
        d.set("skipped_lines", self.meta.skipped_lines as i64);
        d
    }

    // ---------------------------------------------------------------- 静态表

    /// unix 秒 → `YYYY-MM-DD HH:MM:SS`（UTC）。与回放文件名的时间戳配对使用。
    #[func]
    fn format_utc(ts: i64) -> GString {
        GString::from(format_utc(ts).as_str())
    }

    /// 文件名 → unix 时间戳（纯数字才认；带扩展名/空串返回 -1）。
    #[func]
    fn timestamp_from_name(name: GString) -> i64 {
        timestamp_from_name(&name.to_string()).unwrap_or(-1)
    }
}
