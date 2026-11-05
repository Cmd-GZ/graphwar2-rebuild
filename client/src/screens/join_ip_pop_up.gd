extends GwConnectPopup
## JoinIpPopUp（输入 host:port 加入）。实现见 `connect_popups.gd`。

func _init() -> void:
	kind = "join_ip"
