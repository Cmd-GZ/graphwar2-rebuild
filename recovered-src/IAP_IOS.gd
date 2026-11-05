extends Node

signal purchase_confirmed
signal purchase_pending
signal purchase_denied

var root = null
var appstore = null
const GRAPHWAR_SUPPORT_PRODUCT_ID = "graphwar_support"
var support_purchased = false

func purchase_graphwar_support():
	root.log("purchase_graphwar_support")
	if Engine.has_singleton("InAppStore") && appstore != null:
		do_purchase("graphwar_support")

func _ready():
	root = get_node("/root/GraphwarRootNode")

	if Engine.has_singleton("InAppStore"):
		appstore = Engine.get_singleton("InAppStore")
		appstore.set_auto_finish_transaction(true)

		var result = appstore.request_product_info({"product_ids": [GRAPHWAR_SUPPORT_PRODUCT_ID]})
		if result == OK:
			root.log("InAppStore: Successfully started product info request.")
		else:
			root.log("InAppStore: Failed requesting product info.")

		result = appstore.restore_purchases()
		if result == OK:
			root.log("InAppStore: Successfully started restore purchases request.")
		else:
			root.log("InAppStore: Failed requesting restore purchases.")
	else:
		root.log("InAppStore: No app store plugin.")

func _process(delta):
	if appstore != null:
		check_events()

func check_events():
	while appstore.get_pending_event_count() > 0:
		var event = appstore.pop_pending_event()
		root.log("InAppStore event")
		root.log(str(event))

		match event.type:
			"product_info":

				pass
			"purchase":
				if event.product_id == GRAPHWAR_SUPPORT_PRODUCT_ID:
					if event.result == "ok":
						if event.type_code == 0:
							purchase_pending.emit()
						elif event.type_code == 1:
							support_purchased = true
							purchase_confirmed.emit()
						elif event.type_code == 2:
							purchase_denied.emit()
						elif event.type_code == 3:
							support_purchased = true
							purchase_confirmed.emit()
						elif event.type_code == 4:
							purchase_pending.emit()
					elif event.result == "progress":
						purchase_pending.emit()
					else:
						purchase_denied.emit()
			"restore":
				if event.product_id == GRAPHWAR_SUPPORT_PRODUCT_ID:
					if event.result == "ok":
						if event.type_code == 0:
							purchase_pending.emit()
						elif event.type_code == 1:
							support_purchased = true
							purchase_confirmed.emit()
						elif event.type_code == 2:
							purchase_denied.emit()
						elif event.type_code == 3:
							support_purchased = true
							purchase_confirmed.emit()
						elif event.type_code == 4:
							purchase_pending.emit()
					elif event.result == "completed":
						if support_purchased == false:
							purchase_denied.emit()
					else:
						purchase_denied.emit()

func do_purchase(id: String):
	root.log("InAppStore: purchase_graphwar_support")
	appstore.purchase({"product_id": id})
