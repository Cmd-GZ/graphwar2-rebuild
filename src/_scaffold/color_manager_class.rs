//! **不是原版的东西**：把 [`crate::graphwar_color_manager`] 里**已经复原的选色规则**
//! 接成 GDExtension 类 `GwColorManager`，让 `client/src/screens/play_screen.gd` 的
//! 配色段（改前 `TEAM_COLORS` + `assign_colors()`，约 37 行）只留"把结果画到节点上"。
//!
//! # 为什么会有这一份（T58 / `docs/06` §90 的 C 档第 9 段）
//!
//! 原版 `GraphwarColorManager::add_team_color`（x86 `0x464d10` / arm64 `0x49c244`，1052 字节）
//! 已经**整函数逐位**复原在 [`crate::graphwar_color_manager`]（模块文档 §2/§3）：
//!
//! ```text
//! pool = pools[team]; n = pool.len()            ; = 6
//! base = rand::random::<u64>() % n              ; udiv + msub
//! for k in 0..n { c = pool[base]; if c 没被占 { 发(61); used.insert; return }
//!                 base = (base+1) % n }         ; cmp + csinc
//! 兜底: (random::<u8>()%180, %180, %180)        ; and/add/cmp/csel 形状
//! ```
//!
//! `.gd` 改前是这台机器的**第二份实现**，而且少了两样东西：
//!
//! | # | `.gd` 改前 | 真值 |
//! |---|---|---|
//! | 1 | 起点用 Godot `RandomNumberGenerator.randi() % 6` | `rand::random::<u64>() % 6`（RNG 流不可复现；见下"随机流从哪来"） |
//! | 2 | **没有兜底**（6 个色全被占时仍拿到池里的色，可能与别人撞色） | 兜底 = 三个独立 `random::<u8>() % 180` |
//! | 3 | `used` 只在本队内判重 | 占用表是**全局**的（跨队也不撞色） |
//!
//! # 随机流从哪来（这是本文件唯一"不是原版"的设计选择）
//!
//! 原版的 `rand::random()` 是**线程局部 ChaCha 流**，我方客户端复现不了、也不需要复现
//! （原版每局颜色本来就不一样）。所以随机**数**由 `.gd` 用确定性种子 RNG 抽好传进来，
//! **规则**（取模、线性探测、占用表、兜底）留在 Rust —— 即"流是我方选择，算法是复原结论"。
//! 这样两端语义一致、而客户端配色仍然逐局可复现（视觉对拍要求）。
//!
//! # 接口
//!
//! ```gdscript
//! GwColorManager.pool_len()          # 6
//! GwColorManager.random_color_modulo()   # 180
//! GwColorManager.team_colors(team)   # PackedColorArray（6 个；未知队 ⇒ 空）
//! GwColorManager.pick(team, rand_u64, r0, r1, r2, used)   # -> Color
//! GwColorManager.provenance()
//! ```

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_color_manager as cm;

/// 0..=1 的 f32 → 0..=255 的字节（与 `Color8(x,…)` / `Color.from_rgba` 的往返一致）。
fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn to_godot(c: cm::GraphwarColor) -> Color {
    Color::from_rgba(c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0, 1.0)
}

/// "正在被挑色的"那个实体的 id。
///
/// 原版 `add_team_color(entity_id, team)` 的 key 就是实体 id；这里挑色与实体无关
/// （占用表只看 value），所以用一个**不会与 `used` 的键冲突**的哨兵值。
const PICKING_ID: u32 = 0xffff_fffe;

/// 选色规则出口（scaffold；原版是 `GraphwarColorManager` 上的 `add_team_color`）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwColorManager {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwColorManager {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GwColorManager {
    /// 每队的颜色池长度 = **6**（`TEAM_POOL_LEN`，逐位：两张表各六次 push）。
    #[func]
    fn pool_len() -> i64 {
        cm::TEAM_POOL_LEN as i64
    }

    /// 兜底随机色的取模常数 = **180**（`RANDOM_COLOR_MODULO`，逐位：`cmp w8,#0xb4` + `csel`）。
    #[func]
    fn random_color_modulo() -> i64 {
        cm::RANDOM_COLOR_MODULO as i64
    }

    /// 某队的 6 色池（`team` = 0 → `TEAM1_COLORS`，1 → `TEAM2_COLORS`；其它 ⇒ 空）。
    #[func]
    fn team_colors(team: i64) -> PackedColorArray {
        let mut out = PackedColorArray::new();
        let table: Option<&[cm::GraphwarColor; cm::TEAM_POOL_LEN]> = match team {
            0 => Some(&cm::TEAM1_COLORS),
            1 => Some(&cm::TEAM2_COLORS),
            _ => None,
        };
        if let Some(t) = table {
            for c in t.iter() {
                out.push(to_godot(*c));
            }
        }
        out
    }

    /// 按原版规则挑一个颜色：`rand_u64 % 6` 当起点 → **线性探测**取第一个没被占的 →
    /// 6 个都被占时走兜底 `(r0%180, r1%180, r2%180)`。
    ///
    /// * `rand_u64` —— 起点用的随机数（等价原版 `rand::random::<u64>()`，由调用方的
    ///   确定性 RNG 抽好；见文件头"随机流从哪来"）；
    /// * `r0/r1/r2` —— 兜底色的三个随机字节（只在池子被占满时才用到）；
    /// * `used` —— **已经发出去的颜色**（原版占用表的 value 集合；键不参与判定）。
    #[func]
    fn pick(
        team: i64,
        rand_u64: i64,
        r0: i64,
        r1: i64,
        r2: i64,
        used: PackedColorArray,
    ) -> Color {
        let mut mgr = cm::GraphwarColorManager::new();
        // 把"已占用"的颜色灌进占用表。键只是 HashMap 的 key（原版 `used: HashMap<u32, Color>`），
        // 判重看的是 value ⇒ 用 0..n 当键与实体真 id 等价。
        for (i, c) in used.as_slice().iter().enumerate() {
            mgr.set_color(i as u32, cm::GraphwarColor::new(to_byte(c.r), to_byte(c.g), to_byte(c.b)));
        }
        let mut rng = cm::SeqRng::new(
            vec![rand_u64 as u64],
            vec![r0 as u8, r1 as u8, r2 as u8],
        );
        let team_key = team.clamp(0, 255) as u8;
        mgr.add_team_color(PICKING_ID, team_key, &mut rng);
        match mgr.color_of(PICKING_ID) {
            Some(c) => to_godot(c),
            // `add_team_color` 恒返回一个颜色（兜底总会给），这条只是防御。
            None => to_godot(cm::GraphwarColor::new(0, 0, 0)),
        }
    }

    /// 出处与档位（自检、文档复核用；客户端界面不读）。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("pool_len", cm::TEAM_POOL_LEN as i64);
        d.set("random_color_modulo", cm::RANDOM_COLOR_MODULO as i64);
        d.set("add_team_color_addr", &GString::from("0x464d10 (x86) / 0x49c244 (arm64)"));
        d.set("init_team_colors_addr", &GString::from("0x49c660 (arm64, 独立符号)"));
        d.set("team1", &GString::from("(186,42,32)(238,114,0)(125,104,0)(122,160,0)(149,5,62)(234,0,219)"));
        d.set("team2", &GString::from("(107,0,234)(17,0,108)(21,83,221)(0,203,163)(0,116,26)(2,85,63)"));
        d.set(
            "evidence",
            &GString::from(
                "逐位：两张 6 色表由 strh/strb 六次 push 逐元素复算；起点 udiv+msub；\
                 线性探测 cmp+csinc；兜底 and/add/cmp/csel 形状 = %180",
            ),
        );
        d.set("grade", &GString::from("逐位"));
        d
    }
}
