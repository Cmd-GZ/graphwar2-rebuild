extends RefCounted
class_name GwViews
## 原版视图脚本的**挂载点**。
##
## `client/src/views/*.gd` 是 `recovered-src/*.gd` 的**逐字节副本**（从 `Graphwar.pck` 里的
## `.gdc` 反编译回来的原版 GDScript，见 `docs/20-source-recovery.md`）。一份都不许改：
## 要改就说明它"不能直接换"，登记跳过（`docs/16` §40）。
##
## 挂法就是原版的挂法 —— 原版这些脚本是 `.tscn` 里某个节点的 `script`，**只看自己的子节点
## 和视口宽度**，所以在我们代码搭出来的同名同类型节点上 `set_script()` 即可：
##
## ```gdscript
## var vbox := GwUI.vbox("VBoxContainer", GwUI.SEPARATION)   # 先不挂父节点
## ... 填子节点 ...
## GwViews.attach(vbox, "MainMenuButtonResize.gd")           # 挂原版脚本（此时还没进树）
## parent.add_child(vbox)                                    # 进树 → _ready 才会跑
## ```
##
## ⚠️ **必须**在 `add_child()` 之前挂：`_ready` 只在节点进树时发一次；对一个已经进树的节点
## `set_script()` 不会补发 `_ready`，原版脚本就永远不会连 `size_changed`（静默失效）。
##
## ⚠️ 挂上之后这个子树就**归原版管**了：`GwScreen._apply_size_recursive()` 见到
## `GwViews.owns(node)` 就整棵跳过，否则我们那套 `maxi(10, 宽/N)` 会把它盖掉
## （原版**没有**下限，`maxi` 在 105 个原版脚本里出现 0 次，见 `docs/16` §39.6）。

const DIR := "res://src/views/"

## 原版脚本的 `res://` 路径。
static func path_for(view: String) -> String:
	return DIR + view


static func has(view: String) -> bool:
	return ResourceLoader.exists(path_for(view))


## 把原版脚本挂到**还没进树**的节点上，返回该节点（便于链式写）。
## 文件缺失时直接报错 —— 静默跳过会让"换了"和"没换"看起来一样。
static func attach(node: Node, view: String) -> Node:
	var p := path_for(view)
	var s := load(p) as Script
	if s == null:
		push_error("GwViews: 原版脚本缺失 %s" % p)
		return node
	# 已经进树的节点**不会**补发 `_ready`（实测：进树后 set_script，原版脚本一次也不跑，
	# 字号停在主题默认 16）。这里把它变成一条响亮的报错，而不是静默失效。
	if node.is_inside_tree():
		push_error("GwViews: %s 已经在树里，原版脚本 %s 的 _ready 不会跑 —— 先 attach 再 add_child"
			% [node.name, view])
	node.set_script(s)
	return node


## 这个节点是不是由原版视图脚本驱动（⇒ 字号/尺寸由它自己算，我们不再插手）。
static func owns(node: Node) -> bool:
	if node == null:
		return false
	var s := node.get_script() as Script
	if s == null:
		return false
	return String(s.resource_path).begins_with(DIR)
