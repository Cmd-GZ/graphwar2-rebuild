extends Control

signal purchase_confirmed
signal purchase_pending
signal purchase_denied


func _ready():
	$IAP_Android.purchase_confirmed.connect(on_purchase_confirmed)
	$IAP_Android.purchase_pending.connect(on_purchase_pending)
	$IAP_Android.purchase_denied.connect(on_purchase_denied)

	$IAP_IOS.purchase_confirmed.connect(on_purchase_confirmed)
	$IAP_IOS.purchase_pending.connect(on_purchase_pending)
	$IAP_IOS.purchase_denied.connect(on_purchase_denied)


func _process(delta):
	pass

func purchase_graphwar_support():
	print("IAP_holder: purchase_graphwar_support")
	$IAP_Android.purchase_graphwar_support()
	$IAP_IOS.purchase_graphwar_support()

func on_purchase_confirmed():
	purchase_confirmed.emit()

func on_purchase_pending():
	purchase_pending.emit()

func on_purchase_denied():
	purchase_denied.emit()
