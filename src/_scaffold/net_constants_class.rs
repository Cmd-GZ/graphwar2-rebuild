//! **不是原版的东西**：把**原版 Rust 模块里本来就有**的时间常量接给 GDScript。
//!
//! # 为什么会有这一份（T42）
//!
//! 客户端有一处 `.gd` 曾经**自己编了值**，把逐位真值丢掉了（T35 审计揪出来的第一处）：
//!
//! | `.gd`（改前） | 我们编的值 | 原版真值 | 出处（ELF vaddr） | 档位 |
//! |---|---|---|---|---|
//! | `client/src/net/link.gd:36` `connect_timeout := 3.0` | 3 s | **30 s** | `graphwar_client_connection_handler::CONNECT_TIMEOUT_SECS`，arm64 `0x48dccc mov w1,#0x1e` → `0x48dcd0 mov w2,wzr` → `0x48dcd4 bl 0x80b238`（= `std::net::tcp::TcpStream::connect_timeout`，1084 B，arm64 `.symtab` FUNC LOCAL） | **逐位** |
//!
//! 这个常量**已经在**复原的 Rust 模块里（`src/graphwar_client_connection_handler.rs:261`），
//! 所以本任务**不新增任何判据、不改 Rust 语义**，只把"值从哪来"这条线接到 GDScript：
//! `.gd` 里不再出现 `3.0` 这个字面量。
//!
//! # 这是脚手架，不是原版的类
//!
//! 原版 `libgraphwarrust.so` 里**没有** `GwNetConsts` 这个类：连接超时是
//! `GraphwarClientConnectionHandler` 内部的使用点立即数（上表），原版的 GDScript
//! （`recovered-src/*.gd`）也从来不问 Rust 要它。我们的 `client/` 是仿制品
//! （`AGENTS.md` §0.4），所以这里只做**取值出口**：谁想用这个时间常量，就必须经过 Rust，
//! 不许在 GDScript 里再抄一遍数字。
//!
//! # 接口
//!
//! ```gdscript
//! GwNetConsts.connect_timeout_secs()   # 30.0  float（= CONNECT_TIMEOUT_SECS）
//! GwNetConsts.provenance()             # 值 + 出处串（自检/文档复核用）
//! ```
//!
//! 全部是**关联函数**（无 `self`），所以 GDScript 侧直接 `GwNetConsts.xxx()`（与
//! `GwReplayMeta.format_utc()` 同一形态，见 `replay_meta_class.rs`）。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_client_connection_handler as cch;

/// 已复原时间常量的只读取值口（scaffold；原版没有这个类）。
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

    /// 出处自检：值 + 它是从哪个 Rust 常量来的 + 证据地址 + 档位。
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
        d.set("evidence_grade", &GString::from("逐位（arm64 立即数，2.0.60）"));
        d
    }
}
