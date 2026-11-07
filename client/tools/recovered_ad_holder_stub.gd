extends GraphwarAdSwitch
## **试验专用**的 `Ad_holder` 替身（不是原版的东西，原版这个节点是 `Ad_holder.gd` + AdMob 插件）。
##
## 它存在的唯一目的：回答 `dispatch/08-viewport.md` 的问题
## 「原版 `AdContainer.gd` 换不过去，到底缺几个成员？」
##
## `client/tools/recovered/AdContainer.gd`（原版，逐字节未改）自己点名了两个成员：
##
## ```gdscript
## var ad_holder = get_node("/root/GraphwarRootNode/Ad_holder")
## ad_holder.ad_loaded.connect(_update_size)      # ← 成员 1：signal ad_loaded
## self.custom_minimum_size.x = ad_holder.ad_size.x   # ← 成员 2：var ad_size: Vector2i
## ```
##
## 我们注册的 `GraphwarAdSwitch`（`src/_scaffold/graphwar_ad_switch_class.rs`）只有
## `are_ads_enabled()` / `get_ad_unit_id()` / `owned`，**没有**这两个 —— 于是本类补上它们，
## 用 `recovered_view_check.gd` 的 D③ 段证明「补齐之后原版脚本一行不改就能跑」。

## 原版 `Ad_holder.gd` 里 `signal ad_loaded`（广告加载完成后由 AdMob 回调发出）。
signal ad_loaded

## 原版 `Ad_holder.gd` 里 `var ad_size = Vector2i(0, 0)`（AdMob 回的像素尺寸）。
var ad_size: Vector2i = Vector2i(0, 0)
