//! **不是原版的东西**：把**两个在原版 Rust 模块里本来就有**的时间常量接给 GDScript。
//!
//! # 为什么会有这一份（T42）
//!
//! 客户端有两处 `.gd` 曾经**自己编了值**，把逐位真值丢掉了（T35 审计揪出来的）：
//!
//! | `.gd`（改前） | 我们编的值 | 原版真值 | 出处（ELF vaddr） | 档位 |
//! |---|---|---|---|---|
//! | `client/src/net/link.gd:36` `connect_timeout := 3.0` | 3 s | **30 s** | `graphwar_client_connection_handler::CONNECT_TIMEOUT_SECS`，arm64 `0x48dccc mov w1,#0x1e` → `0x48dcd0 mov w2,wzr` → `0x48dcd4 bl 0x80b238`（= `std::net::tcp::TcpStream::connect_timeout`，1084 B，arm64 `.symtab` FUNC LOCAL） | **逐位** |
//! | `client/src/net/discovery.gd:35` `ROOM_TTL_MS := 12000` | 12 s | **10 s** | `graphwar_lan_broadcaster_v6::ENTRY_TTL_SECS`，arm64 `0x3de6d4 bl 0x80b0e0`(`Instant::now`) → `0x3de6e0 bl 0x81f044`(`Sub for Instant`) → `0x3de6e4 cmp x0,#0xa` + `0xb.lo 0x3de68c`（`secs < 10` 保留，否则落下去删） | **逐位** |
//!
//! 两个常量**都已经**在复原的 Rust 模块里（`src/graphwar_client_connection_handler.rs:261`、
//! `src/graphwar_lan_broadcaster_v6.rs:154`），所以本任务**不新增任何判据、不改 Rust 语义**，
//! 只把"值从哪来"这条线接到 GDScript：`.gd` 里不再出现 `3.0` / `12000` 字面量。
//!
//! # 这是脚手架，不是原版的类
//!
//! 原版 `libgraphwarrust.so` 里**没有** `GwNetConsts` 这个类：那两个常量是
//! `GraphwarClientConnectionHandler` / `GraphwarLanBroadcasterV6` 内部的使用点立即数
//! （见上表），原版的 GDScript（`recovered-src/*.gd`）也从来不问 Rust 要它们。
//! 我们的 `client/` 是仿制品（`AGENTS.md` §0.4），所以这里只做**取值出口**：
//! 谁想用这两个时间常量，就必须经过 Rust，不许在 GDScript 里再抄一遍数字。
//!
//! # 接口
//!
//! ```gdscript
//! GwNetConsts.connect_timeout_secs()   # 30.0  float（= CONNECT_TIMEOUT_SECS）
//! GwNetConsts.entry_ttl_ms()           # 10000  int（= ENTRY_TTL_SECS × 1000）
//! GwNetConsts.entry_ttl_secs()         # 10.0
//! GwNetConsts.broadcast_period_secs()  # 1.0（同一函数的广播周期闸，arm64 0x3de7c8–0x3de7d4）
//! GwNetConsts.provenance()             # 上表那几个字段 + 出处串（自检/文档复核用）
//! ```
//!
//! 全部是**关联函数**（无 `self`），所以 GDScript 侧直接 `GwNetConsts.xxx()`（与
//! `GwReplayMeta.format_utc()` 同一形态，见 `replay_meta_class.rs`）。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_client_connection_handler as cch;
use crate::graphwar_lan_broadcaster_v6 as lan;

/// 两个**已复原**时间常量的只读取值口（scaffold；原版没有这个类）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwNetConsts {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwNetConsts {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GwNetConsts {
    /// 单次 TCP 连接尝试的超时，**秒**。
    ///
    /// 值 = [`cch::CONNECT_TIMEOUT_SECS`]（`30`）。**逐位**证据见文件头表：
    /// arm64 `0x48dccc mov w1,#0x1e`（`0x1e` = 30，`w1` = `Duration.secs`）+
    /// `0x48dcd0 mov w2,wzr`（`w2` = nanos = 0）+ `0x48dcd4 bl std::net::tcp::TcpStream::connect_timeout`。
    ///
    /// ⚠️ 原版**没有**重连退避、没有心跳周期：这是连接线程上**唯一**的时间常量
    /// （`CONNECT_TIMEOUT_SECS` 的文档）。
    #[func]
    fn connect_timeout_secs() -> f64 {
        cch::CONNECT_TIMEOUT_SECS as f64
    }

    /// LAN 房间条目（收到的广播）的存活时间，**毫秒** —— `ENTRY_TTL_SECS × 1000`。
    ///
    /// 值 = [`lan::ENTRY_TTL_SECS`]（`10`）。**逐位**证据见文件头表：
    /// `secs >= 10` 就删（`0x3de6e4 cmp x0,#0xa` + `0xb.lo` 保留）。
    ///
    /// 为什么给毫秒：`client/src/net/discovery.gd` 的时钟是**毫秒**累加器
    /// （`_clock_ms`，由 `poll(delta_ms)` 推进，不读墙钟），换单位在 Rust 侧做一次，
    /// GDScript 里就不必再写 `* 1000`（那也是一个"我们的数"）。
    #[func]
    fn entry_ttl_ms() -> i64 {
        (lan::ENTRY_TTL_SECS * 1000) as i64
    }

    /// 同上，**秒**（浮点；给需要秒的口径用，例如与抓包时间戳对照）。
    #[func]
    fn entry_ttl_secs() -> f64 {
        lan::ENTRY_TTL_SECS as f64
    }

    /// 广播周期，**秒**：`process` 里 `elapsed > 1 s` 才发下一包（`BROADCAST_PERIOD_SECS`）。
    ///
    /// **逐位**：arm64 `0x3de7b8 bl Instant::now` → `0x3de7c4 bl Sub for Instant` →
    /// `0x3de7c8 cbz x0`（`secs == 0` ⇒ 不发）→ `0x3de7cc cmp x0,#1` / `0x3de7d0 b.ne`（`secs != 1` ⇒ 发）
    /// → `0x3de7d4 cbz w1`（`secs == 1 且 nanos == 0` ⇒ 不发）三条合起来 = `elapsed > 1 s`。
    #[func]
    fn broadcast_period_secs() -> f64 {
        lan::BROADCAST_PERIOD_SECS as f64
    }

    /// 出处自检：每个值 + 它是从哪个 Rust 常量来的 + 证据地址 + 档位。
    ///
    /// 只给**自检/文档复核**用（客户端界面不读它）。键名固定，见返回值。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("connect_timeout_secs", cch::CONNECT_TIMEOUT_SECS as f64);
        d.set(
            "connect_timeout_source",
            &GString::from("graphwar_client_connection_handler::CONNECT_TIMEOUT_SECS"),
        );
        d.set(
            "connect_timeout_evidence",
            &GString::from(
                "arm64 0x48dccc mov w1,#0x1e (Duration.secs=30) + 0x48dcd0 mov w2,wzr (nanos=0) \
                 + 0x48dcd4 bl 0x80b238 = std::net::tcp::TcpStream::connect_timeout",
            ),
        );
        d.set("entry_ttl_secs", lan::ENTRY_TTL_SECS as f64);
        d.set("entry_ttl_ms", (lan::ENTRY_TTL_SECS * 1000) as i64);
        d.set(
            "entry_ttl_source",
            &GString::from("graphwar_lan_broadcaster_v6::ENTRY_TTL_SECS"),
        );
        d.set(
            "entry_ttl_evidence",
            &GString::from(
                "arm64 0x3de6d4 bl Instant::now + 0x3de6e0 bl Sub for Instant + \
                 0x3de6e4 cmp x0,#0xa + 0x3de6e8 b.lo => secs >= 10 删",
            ),
        );
        d.set("broadcast_period_secs", lan::BROADCAST_PERIOD_SECS as f64);
        d.set(
            "broadcast_period_evidence",
            &GString::from(
                "arm64 0x3de7c8 cbz x0 / 0x3de7cc cmp x0,#1 / 0x3de7d0 b.ne / 0x3de7d4 cbz w1 \
                 => elapsed > 1 s",
            ),
        );
        d.set("evidence_grade", &GString::from("逐位（arm64 立即数/比较指令，2.0.60）"));
        d
    }
}
