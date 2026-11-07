//! 贴图饿汉加载：`GraphwarTextureLoader` —— **三张 `HashMap<String, Gd<Texture2D>>`**
//! 加 119 条 `res://*.svg`（其中 91 个互不相同的 asset）。
//!
//! 本模块把三件事钉死：**它是个独立模块**（不在 `graphics_manager` 里）、
//! **只有 1 个可归属函数是错的**（arm64 有 **4** 个方法，见 §1）、
//! **名字 → 路径的完整表**（§4，从 `.rodata` 逐条抠出来）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（4 个，**不是 1 个**）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 职责 | 等级 |
//! |---|---|---|---|---|
//! | `new` | `0x40c2f4` / **21212** | `0x4d3180` / 15936（推断，见下） | 饿汉加载全部贴图 | 观测 |
//! | `get_skin_texture` | `0x40c1cc` / 296 | 无锚 | `HashMap<String, Gd<Texture2D>>` 查表（`+0x150`） | 逐位 |
//! | `get_face_texture` | `0x40c0a4` / 296 | 无锚 | 同上（`+0x180`） | 逐位 |
//! | `get_hat_texture` | `0x40bf7c` / 296 | 无锚 | 同上（`+0x1b0`） | 逐位 |
//!
//! ⚠️ **`docs/coverage-matrix.tsv` 记的「1 个可归属函数」是 `panic::Location` 口径的下限**：
//! 这个模块只有一个 Location（`graphwar_texture_loader.rs:145:47`，落在 `new` 里），
//! 另外 3 个 getter **一次都不 panic**，所以按 Location 数只能是 1。
//! arm64 未 strip 的 `.symtab` 里有 **4 个** `GraphwarTextureLoader::*` 符号
//! ⇒ 模块规模是 4，矩阵那个 1 不是「只有 1 个」。
//!
//! x86 侧 `0x4d3180` 的归属是**推断**：`:145:47` 这枚 Location 在 x86 被
//! 4 个函数引用（`0x4c1ae0` / `0x4d1f10` / `0x4d2600` / `0x4d3180`），
//! 说明那行代码被内联进了 4 个宿主；其中尺寸最接近（15936 / 21212 = 0.75，
//! 在闸内）且引用最多贴图路径的就是 `0x4d3180`。
//!
//! ## 2. 数据结构（逐位）
//!
//! ```text
//! GraphwarTextureLoader（GDExtension 类实例，前 0x150 字节是基类数据）
//!   +0x150 : HashMap<String, Gd<Texture2D>>   ; skin    （RawTable 0x20 + hasher 0x10）
//!   +0x180 : HashMap<String, Gd<Texture2D>>   ; face
//!   +0x1b0 : HashMap<String, Gd<Texture2D>>   ; hat
//! ```
//!
//! 三个 getter 的形状**完全一致**（各 296 字节）：`hash_one(&name)` →
//! 桶里 `memcmp` 比 key（所以 key 是 `String`，不是 `u32`）→ 命中
//! `RawGd::clone` 返回 `Some`，未命中返回 `None`（`Option` 的 niche：
//! 命中写 1 + 两个 8 字节，未命中写 0）。
//!
//! * `get_skin_texture` 读 `[x0+0x168]` 判空、`add x0,x0,#0x170` 取 hasher ⇒ 表在 `+0x150`；
//! * `get_face_texture` 读 `[x0+0x198]` / `+0x1a0` ⇒ 表在 `+0x180`；
//! * `get_hat_texture` 读 `[x0+0x1c8]` / `+0x1d0` ⇒ 表在 `+0x1b0`。
//!
//! `+0x18` 偏移的差是 `RawTable.items` 与 `hasher` 在 `HashMap` 里的固定位置
//! （`items` = 表 + 0x18，`hasher` = 表 + 0x20），三处一致 ⇒ 不是三个不同结构。
//!
//! ## 3. 为什么它不在 `graphics_manager` 里（回顾）
//!
//! `graphics_manager` 管的是 `player_id → 名字/脸/帽` **三张 id 表**（`+0x18/+0x48/+0x78`，
//! `docs/16` §28）；本模块的三张表是**名字 → 贴图**，键类型不同（`String` vs `u32`）、
//! 偏移不同、函数也不同。⇒ 独立模块，这个结论本轮用「键类型 + 偏移 + 方法名」三件事复核过。
//!
//! ## 4. 名字 → 路径全表（逐位，`.rodata` 逐条抠）
//!
//! `new` 里 119 次「名字 + `res://...svg`」成对出现。下面按目录分类列出
//! （**已去重**；完整含重复的表见 [`TEXTURES`]）：
//!
//! | 目录 | 条数 | 名字 |
//! |---|---|---|
//! | `res://skins/` | 23 | blue gray pink purple red **target** white_skin yellow green light_green true_purple brown calico wargod arrow face_paint_1 face_paint_2 bot_0…bot_5 |
//! | `res://faces/` | 17 | no_face anime_eyes bot_0 regular_eyes blue_eyes green_eyes purple_eyes aviator_glare cute_eyes nerd_glasses mustache baggy_eyes asian cat_eyes eye_patch baby_eyes alien_eyes |
//! | `res://hats/` | 29 | no_hat soldier_helmet mohawk mohawk_red cowboy straw_hat blonde_boy blonde_girl brunette_girl powdered_wig brown_boy witch_hat hood horned_helmet hair_band cat_ears unicorn roman_helmet chef_hat top_hat tricone asian_hat mad_hair baseball_cap crown anime_hair ponytail antenna_0 |
//! | `res://ranks/` | 22 | rank_0 … rank_21 |
//! | `res://`（UI 图标） | 28 | axis_option_0…3 fx_option dx_dy_option empty_set mute unmute play_option pause_option lock unlock turn_30s turn_1m turn_2m turn_3m turn_5m turn_inf turns_sequential turns_simultaneous no_chat view_chat no_chat_new_chat view_names hide_names view_recent_functions view_all_functions |
//!
//! **原版有一处笔误**：`res://hats//no_hat.svg`（双斜杠）与 `res://hats/no_hat.svg`
//! **两条都在**（`docs/16` §29）。我们**不修**——修了就与真值不同。
//!
//! ## 5. 未知
//!
//! * 三张表只覆盖 skin/face/hat 三类；`ranks/` 与 UI 图标的查询入口**没有找到**
//!   （它们同样在 `new` 里被加载，但没有任何方法按名字取它们）⇒ 存储位置**未知**；
//! * `new` 里 15936/21212 字节中的**加载胶水**（`ResourceLoader` 调用、失败回退）
//!   只看出「逐个 `load` 后插表」，失败时的默认贴图**没有解**；
//! * `get_*_texture` 的 `Option` 未命中有没有回退到 `no_face` / `no_hat` 这类兜底：
//!   **没有**——未命中直接返回 `None`，兜底在调用方（`entity_drawer`）。

/// 一条贴图登记：名字（HashMap 的键）与资源路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureEntry {
    pub name: &'static str,
    pub path: &'static str,
}

/// 三张表的分工。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureKind {
    /// `+0x150`
    Skin,
    /// `+0x180`
    Face,
    /// `+0x1b0`
    Hat,
}

/// 三张表的偏移（逐位，见模块文档 §2）。
pub const SKIN_MAP_OFFSET: usize = 0x150;
pub const FACE_MAP_OFFSET: usize = 0x180;
pub const HAT_MAP_OFFSET: usize = 0x1b0;

/// `get_*_texture` 的字节数（三个都是 296）。
pub const GETTER_SIZE: usize = 296;

/// `new` 的字节数（arm64）。
pub const NEW_SIZE: usize = 21212;

/// 皮肤（23）。
pub const SKINS: [&str; 23] = [
    "blue", "gray", "pink", "purple", "red", "target", "white_skin", "yellow", "green",
    "light_green", "true_purple", "brown", "calico", "wargod", "arrow", "face_paint_1",
    "face_paint_2", "bot_0", "bot_1", "bot_2", "bot_3", "bot_4", "bot_5",
];

/// 脸（17）。
pub const FACES: [&str; 17] = [
    "no_face", "anime_eyes", "bot_0", "regular_eyes", "blue_eyes", "green_eyes", "purple_eyes",
    "aviator_glare", "cute_eyes", "nerd_glasses", "mustache", "baggy_eyes", "asian", "cat_eyes",
    "eye_patch", "baby_eyes", "alien_eyes",
];

/// 帽子（28 个互不相同的名字；原版对 `no_hat` 登记了**两条**路径，见
/// [`NO_HAT_TYPO_PATH`]）。
pub const HATS: [&str; 28] = [
    "no_hat", "soldier_helmet", "mohawk", "mohawk_red", "cowboy", "straw_hat", "blonde_boy",
    "blonde_girl", "brunette_girl", "powdered_wig", "brown_boy", "witch_hat", "hood",
    "horned_helmet", "hair_band", "cat_ears", "unicorn", "roman_helmet", "chef_hat", "top_hat",
    "tricone", "asian_hat", "mad_hair", "baseball_cap", "crown", "anime_hair", "ponytail",
    "antenna_0",
];

/// 三张表之外的资产（加载了但没有查询入口，模块文档 §5）。
pub const RANKS: [&str; 22] = [
    "rank_0", "rank_1", "rank_2", "rank_3", "rank_4", "rank_5", "rank_6", "rank_7", "rank_8",
    "rank_9", "rank_10", "rank_11", "rank_12", "rank_13", "rank_14", "rank_15", "rank_16",
    "rank_17", "rank_18", "rank_19", "rank_20", "rank_21",
];

/// UI 图标（28）。
pub const UI_ICONS: [&str; 28] = [
    "axis_option_0", "axis_option_1", "axis_option_2", "axis_option_3", "fx_option",
    "dx_dy_option", "empty_set", "mute", "unmute", "play_option", "pause_option", "lock",
    "unlock", "turn_30s", "turn_1m", "turn_2m", "turn_3m", "turn_5m", "turn_inf",
    "turns_sequential", "turns_simultaneous", "no_chat", "view_chat", "no_chat_new_chat",
    "view_names", "hide_names", "view_recent_functions", "view_all_functions",
];

/// 名字 → 路径（`res://<dir>/<name>.svg`）。
pub fn path_of(kind: TextureKind, name: &str) -> String {
    let dir = match kind {
        TextureKind::Skin => "skins",
        TextureKind::Face => "faces",
        TextureKind::Hat => "hats",
    };
    format!("res://{}/{}.svg", dir, name)
}

/// 原版 `.rodata` 里确实出现过的**第二条** `no_hat` 路径（双斜杠笔误）。
pub const NO_HAT_TYPO_PATH: &str = "res://hats//no_hat.svg";

/// 贴图加载器：三张表。
///
/// 原版的表值是 `Gd<Texture2D>`（16 字节 `RawGd`）；本 crate 不链 Godot，
/// 所以值退化成「路径字符串」——**表结构、键、命中/未命中语义都保持原样**。
#[derive(Debug, Default)]
pub struct GraphwarTextureLoader {
    pub skin: std::collections::HashMap<String, String>,
    pub face: std::collections::HashMap<String, String>,
    pub hat: std::collections::HashMap<String, String>,
}

impl GraphwarTextureLoader {
    /// `new`（arm64 `0x40c2f4`，21212 字节）：把三类贴图全部按名字装进表。
    ///
    /// 原版在这里做「`load` SVG → 插表」，失败分支未解（§5）。
    pub fn new() -> Self {
        let mut me = Self::default();
        for n in SKINS {
            me.skin.insert(n.to_string(), path_of(TextureKind::Skin, n));
        }
        for n in FACES {
            me.face.insert(n.to_string(), path_of(TextureKind::Face, n));
        }
        for n in HATS {
            me.hat.insert(n.to_string(), path_of(TextureKind::Hat, n));
        }
        me
    }

    /// `get_skin_texture`（arm64 `0x40c1cc`，表 `+0x150`）。
    pub fn get_skin_texture(&self, name: &str) -> Option<&String> {
        self.skin.get(name)
    }

    /// `get_face_texture`（arm64 `0x40c0a4`，表 `+0x180`）。
    pub fn get_face_texture(&self, name: &str) -> Option<&String> {
        self.face.get(name)
    }

    /// `get_hat_texture`（arm64 `0x40bf7c`，表 `+0x1b0`）。
    pub fn get_hat_texture(&self, name: &str) -> Option<&String> {
        self.hat.get(name)
    }

    /// 三张表的总条数。
    pub fn len(&self) -> usize {
        self.skin.len() + self.face.len() + self.hat.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
