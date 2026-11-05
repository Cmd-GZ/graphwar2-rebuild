extends Control

var root = null
var ad_view: AdView
var ad_size = Vector2i(0, 0)

signal ad_loaded

func _ready():
	root = get_node("/root/GraphwarRootNode")


	MobileAds.initialize()
	create_ad_view()


func create_ad_view() -> void :
	var ad_switch = get_node("GraphwarAdSwitch")

	if ad_switch.are_ads_enabled():
		root.log("create_ad_view")

		if ad_view:
			destroy_ad_view()

		var unit_id = ad_switch.get_ad_unit_id()
		ad_view = AdView.new(unit_id, AdSize.FULL_BANNER, AdPosition.Values.BOTTOM)

		register_ad_listener()

		var ad_request: = AdRequest.new()
		ad_view.load_ad(ad_request)

func destroy_ad_view() -> void :
	if ad_view:
		root.log("destroy_ad_view")


		ad_view.destroy()
		ad_view = null
		ad_size = Vector2i(0, 0)

func register_ad_listener() -> void :
	if ad_view != null:
		root.log("register_ad_listener")

		var ad_listener: = AdListener.new()

		ad_listener.on_ad_failed_to_load = func(load_ad_error: LoadAdError) -> void :
			root.log("_on_ad_failed_to_load: " + load_ad_error.message)
		ad_listener.on_ad_clicked = func() -> void :
			root.log("_on_ad_clicked")
		ad_listener.on_ad_closed = func() -> void :
			root.log("_on_ad_closed")
		ad_listener.on_ad_impression = func() -> void :
			root.log("_on_ad_impression")
		ad_listener.on_ad_loaded = func() -> void :
			root.log("_on_ad_loaded")
			root.log(str(ad_view.get_width()) + " " + str(ad_view.get_height()))
			root.log(str(ad_view.get_width_in_pixels()) + " " + str(ad_view.get_height_in_pixels()))
			ad_size = Vector2i(ad_view.get_width_in_pixels(), ad_view.get_height_in_pixels())
			ad_loaded.emit()
		ad_listener.on_ad_opened = func() -> void :
			root.log("_on_ad_opened")

		ad_view.ad_listener = ad_listener
