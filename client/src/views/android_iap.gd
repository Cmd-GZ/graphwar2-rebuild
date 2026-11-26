extends Node

signal purchase_confirmed
signal purchase_pending
signal purchase_denied


enum PurchaseState{
	UNSPECIFIED_STATE = 0, 
	PURCHASED = 1, 
	PENDING = 2, 
}


enum BillingResponseCode{
	SERVICE_TIMEOUT = -3, 
	FEATURE_NOT_SUPPORTED = -2, 
	SERVICE_DISCONNECTED = -1, 
	OK = 0, 
	USER_CANCELED = 1, 
	SERVICE_UNAVAILABLE = 2, 
	BILLING_UNAVAILABLE = 3, 
	ITEM_UNAVAILABLE = 4, 
	DEVELOPER_ERROR = 5, 
	ERROR = 6, 
	ITEM_ALREADY_OWNED = 7, 
	ITEM_NOT_OWNED = 8, 
	NETWORK_ERROR = 12
}



const ITEM_ACKNOWLEDGED: Array = ["graphwar_support"]



var root = null
var billing = null

func purchase_graphwar_support():
	root.log("IAP_Android: purchase_graphwar_support")
	if Engine.has_singleton("AndroidIAPP") && billing != null:
		do_purchase("graphwar_support")


func _ready():
	root = get_node("/root/GraphwarRootNode")

	await get_tree().create_timer(1).timeout
	run_iapp_billing()

func run_iapp_billing():
	if Engine.has_singleton("AndroidIAPP"):

		billing = Engine.get_singleton("AndroidIAPP")
		root.log("AndroidIAPP singleton loaded")




		billing.helloResponse.connect(_on_hello_response)

		billing.startConnection.connect(_on_start_connection)

		billing.connected.connect(_on_connected)

		billing.disconnected.connect(_on_disconnected)




		billing.query_purchases.connect(_on_query_purchases)

		billing.query_purchases_error.connect(_on_query_purchases_error)




		billing.query_product_details.connect(query_product_details)

		billing.query_product_details_error.connect(_on_query_product_details_error)




		billing.purchase.connect(_on_purchase)

		billing.purchase_error.connect(_on_purchase_error)




		billing.purchase_updated.connect(_on_purchase_updated)

		billing.purchase_cancelled.connect(_on_purchase_cancelled)

		billing.purchase_update_error.connect(_on_purchase_update_error)




		billing.purchase_consumed.connect(_on_purchase_consumed)

		billing.purchase_consumed_error.connect(_on_purchase_consumed_error)




		billing.purchase_acknowledged.connect(_on_purchase_acknowledged)

		billing.purchase_acknowledged_error.connect(_on_purchase_acknowledged_error)


		billing.startConnection()
	else:
		root.log("AndroidIAPP singleton not found")

func _on_start_connection() -> void :
	root.log("Billing: start connection")

func _on_connected() -> void :
	root.log("Billing successfully connected")
	await get_tree().create_timer(0.4).timeout
	if billing.isReady():



		billing.queryProductDetails(ITEM_ACKNOWLEDGED, "inapp")





		billing.queryPurchases("inapp")


func _on_disconnected() -> void :
	root.log("Billing disconnected")


func _on_hello_response(response) -> void :
	root.log("Hello signal response: " + response)


func query_product_details(response) -> void :
	root.log("on_query_product_details: ")
	for i in response["product_details_list"].size():
		var product = response["product_details_list"][i]


		root.log(str(product))





func _on_query_purchases(response) -> void :
	root.log("on_query_Purchases_response: ")
	for purchase in response["purchases_list"]:
		process_purchase(purchase)

	if response["purchases_list"].size() == 0:
		root.log("purchase_denied")
		purchase_denied.emit()


func _on_purchase_updated(response):
	for purchase in response["purchases_list"]:
		process_purchase(purchase)



func process_purchase(purchase):
	root.log("process_purchase: ")
	root.log(str(purchase));

	var is_graphwar_support = false
	for product in purchase["products"]:
		if (product == "graphwar_support"):
			is_graphwar_support = true
			break

	if is_graphwar_support == false:
		return

	if purchase["purchase_state"] == PurchaseState.PURCHASED:
		if not purchase["is_acknowledged"]:
			root.log("Acknowledging: " + purchase["purchase_token"])
			billing.acknowledgePurchase(purchase["purchase_token"])
		root.log("purchase_confirmed")
		purchase_confirmed.emit()
	elif purchase["purchase_state"] == PurchaseState.PENDING:
		root.log("purchase_pending")
		purchase_pending.emit()
	else:
		root.log("purchase_denied")
		purchase_denied.emit()


func do_purchase(id: String, is_personalized: bool = false):
	billing.purchase([id], is_personalized)


func do_subsciption(subscription_id: String, base_plan_id: String, is_personalized: bool = false):
	billing.subscribe([subscription_id], [base_plan_id], is_personalized)


func print_purchases(purchases):
	for purchase in purchases:
		root.log(JSON.stringify(purchase, "  "))


func _on_purchase(response) -> void :
	root.log("Purchase started:")
	root.log(JSON.stringify(response, "  "))


func _on_purchase_cancelled(response) -> void :
	root.log("Purchase_cancelled:")
	root.log(JSON.stringify(response, "  "))


func _on_purchase_consumed(response) -> void :
	root.log("Purchase_consumed:")
	root.log(JSON.stringify(response, "  "))


func _on_purchase_acknowledged(response) -> void :
	root.log("Purchase_acknowledged:")
	root.log(JSON.stringify(response, "  "))


func _on_purchase_update_error(error) -> void :
	root.log(JSON.stringify(error, "  "))


func _on_purchase_error(error) -> void :
	root.log(JSON.stringify(error, "  "))


func _on_purchase_consumed_error(error) -> void :
	root.log(JSON.stringify(error, "  "))


func _on_purchase_acknowledged_error(error) -> void :
	root.log(JSON.stringify(error, "  "))


func _on_query_purchases_error(error) -> void :
	root.log(JSON.stringify(error, "  "))


func _on_query_product_details_error(error) -> void :
	root.log(JSON.stringify(error, "  "))
