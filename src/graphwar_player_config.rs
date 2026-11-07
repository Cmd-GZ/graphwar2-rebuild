//! 原版 `src/graphwar_player_config.rs` —— **玩家外观档案：名字 / 皮肤 / 脸 / 帽子**。
//!
//! ## 定性（2026-11-07，`docs/16-client.md` §30.7，`观测`）
//!
//! 8 个函数，2.0.60 arm64 符号表全给出来了（`docs/02-module-map.md` §18.4 只记了
//! 8 个方法的**名字**，这次把体量与调用关系也钉住了）：
//!
//! | 符号 | arm64 | 字节 | 谁调 |
//! |---|---|---|---|
//! | `new` | `0x3914f4` | 736 | `<GraphwarRootNode as INode>::init` |
//! | `load` | `0x3917d4` | 2676 | 只有 `new` |
//! | `save` | `0x392388` | 1500 | `set_face` / `set_hat` / `set_skin` / `RootNode::process_button_pressed` / `PlayerShowcase::on_text_change` |
//! | `lock` | `0x392248` | 320 | `RootNode::purchase_denied` + Godot 方法分发（`handle_fallible_call`） |
//! | `unlock` | `0x392964` | 212 | （未观测到直接调用者，见下） |
//! | `set_skin` | `0x392be8` | 216 | （未观测到直接调用者） |
//! | `set_face` | `0x392b10` | 216 | （未观测到直接调用者） |
//! | `set_hat` | `0x392a38` | 216 | （未观测到直接调用者） |
//!
//! **`set_*` 没有直接调用者**不是"死代码"：`lock` 的调用者里有
//! `godot_core::private::handle_fallible_call` —— 那是 gdext **导出方法**的分发入口，
//! `set_skin`/`set_face`/`set_hat` 是**暴露给 GDScript 的**（`.gd` 里点按钮调），
//! 所以走的是函数指针/导出表，`bl` 图看不见。这与 `AGENTS.md` 的"调用图会骗人"完全吻合。
//!
//! ## 落盘（`逐位`：`FileAccess` 调用序列）
//!
//! * `load` → `Os::singleton` ×4、`FileAccess::open` 1 次、`get_as_text` 1 次、
//!   `str::trim_matches` ×2、`String::clone` ×7、`check_rtti` ×8、`unwrap_failed` ×3；
//! * `save` → `FileAccess::open` 1 次、**`store_string` 1 次**、`close` 1 次、
//!   `RefCounted::unreference` 1 次。
//!
//! ⇒ 存档是**一个文本文件、一次性 `store_string` 写完整串**（不是 JSON 序列化：
//! 本模块没有 serde 符号），格式未解（标 `未知`；`trim_matches` 说明按行/按分隔符切）。
//!
//! ## 字段偏移（上一轮 `docs/16-client.md` §26 从 `player_screen_control` 读出的）
//!
//! `PlayerShowcase` 读这份配置时用的偏移：名字 `+0x00`、皮肤 `+0x60`、
//! 脸 `+0x78`、帽 `+0x90`、变更计数 `+0xa8`。
//! 注意 `+0x60` 之前有 96 字节 —— 那一段装的是**解锁表**（`lock`/`unlock` 动的就是它），
//! 与"名字"字段合起来正好解释 8 个函数里 2 个专管锁。
//!
//! ## `lock` 的调用者为什么是 `purchase_denied`
//!
//! 内购失败 ⇒ 把刚解锁的东西**锁回去**（`RootNode::purchase_denied` ← AdMob 插件回调，
//! 见 `docs/16-client.md` §20 的 `graphwar_ad_switch`）。这条把"外观解锁"与
//! "内购"接上了，是本模块唯一的对外因果。

/// 变更计数（`+0xa8`）的语义：**每次 `save` 之前**由 `set_*` 自增，
/// `PlayerShowcase` 用它判断"要不要重画"（`推断`：计数没有被序列化，
/// 因为 `save` 只写一帧快照，且 `load` 里没有对应字段）。
pub const CHANGE_COUNTER_OFFSET: usize = 0xa8;

/// 原版字段偏移（`docs/16-client.md` §26，`观测`）。
pub const NAME_OFFSET: usize = 0x00;
pub const SKIN_OFFSET: usize = 0x60;
pub const FACE_OFFSET: usize = 0x78;
pub const HAT_OFFSET: usize = 0x90;

/// 玩家外观档案。
///
/// 字段**顺序无关**（原版是按偏移访问的，不是按元组），这里按语义排。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphwarPlayerConfig {
    /// `+0x00`：玩家名字（空串 = 未设置，界面会显示 `PlayerShowcase` 的占位）。
    pub name: String,
    /// `+0x60`：皮肤名（对应 `res://skins/*.svg` 的 stem）。
    pub skin: String,
    /// `+0x78`：脸名（`res://faces/*.svg`）。
    pub face: String,
    /// `+0x90`：帽子名（`res://hats/*.svg`）。
    pub hat: String,
    /// 已解锁项集合（`+0x10..0x60` 那一段的抽象：原版是一个固定长度表）。
    pub unlocked: Vec<String>,
    /// `+0xa8`：变更计数。
    pub change_counter: u32,
}

impl GraphwarPlayerConfig {
    /// 对应 `new`：`new` 里**马上**调 `load`（`bl` 图里 `load` 的唯一调用者），
    /// 所以"构造 = 读盘"，没有"默认值 + 稍后载入"两段式。
    pub fn new(loaded: Option<GraphwarPlayerConfig>) -> Self {
        loaded.unwrap_or_default() // 原版 load 失败时的兜底未观测，标 `推断`
    }

    /// 对应 `set_skin` / `set_face` / `set_hat`：改一个字段、计数 +1、
    /// 然后**立刻 `save`**（三个 setter 都 `bl` 到 `save`，这是 `观测`）。
    pub fn set_skin(&mut self, skin: impl Into<String>) {
        self.skin = skin.into();
        self.change_counter = self.change_counter.wrapping_add(1);
    }

    pub fn set_face(&mut self, face: impl Into<String>) {
        self.face = face.into();
        self.change_counter = self.change_counter.wrapping_add(1);
    }

    pub fn set_hat(&mut self, hat: impl Into<String>) {
        self.hat = hat.into();
        self.change_counter = self.change_counter.wrapping_add(1);
    }

    /// 对应 `lock`（内购失败回滚用）。
    pub fn lock(&mut self, item: &str) -> bool {
        let before = self.unlocked.len();
        self.unlocked.retain(|x| x != item);
        self.unlocked.len() != before
    }

    /// 对应 `unlock`（212 字节，比 `lock` 短 108 字节 —— 它只是 `push`，
    /// 没有 `lock` 里的"回滚已选外观"逻辑；`观测`：两者体量差就是这条）。
    pub fn unlock(&mut self, item: impl Into<String>) {
        let item = item.into();
        if !self.unlocked.contains(&item) {
            self.unlocked.push(item);
        }
    }

    /// 对应 `save`：**一次性 `store_string`**（`逐位`，见模块文档）。
    ///
    /// 这里输出成每行 `键=值` 的形态；真实分隔符**未知**，
    /// 不要拿它去和原版存档对拍。
    pub fn save(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("name={}\n", self.name));
        out.push_str(&format!("skin={}\n", self.skin));
        out.push_str(&format!("face={}\n", self.face));
        out.push_str(&format!("hat={}\n", self.hat));
        for item in &self.unlocked {
            out.push_str(&format!("unlocked={item}\n"));
        }
        out
    }

    /// 对应 `load`：`get_as_text` 后按行 `trim_matches` 切。
    pub fn load(text: &str) -> Self {
        let mut cfg = GraphwarPlayerConfig::default();
        for line in text.lines() {
            let line = line.trim();
            let (Some((k, v)), true) = (line.split_once('='), !line.is_empty()) else {
                continue;
            };
            match k {
                "name" => cfg.name = v.to_string(),
                "skin" => cfg.skin = v.to_string(),
                "face" => cfg.face = v.to_string(),
                "hat" => cfg.hat = v.to_string(),
                "unlocked" => cfg.unlocked.push(v.to_string()),
                _ => {}
            }
        }
        cfg
    }
}
