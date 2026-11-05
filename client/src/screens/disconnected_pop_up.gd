extends GwConnectPopup
## DisconnectedPopUp（"You have been disconnected."）。实现见 `connect_popups.gd`。

func _init() -> void:
	kind = "disconnected"
