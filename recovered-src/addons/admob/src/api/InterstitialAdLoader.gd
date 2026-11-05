





















class_name InterstitialAdLoader
extends MobileSingletonPlugin

static var _plugin = _get_plugin("PoingGodotAdMobInterstitialAd")

var ad_unit_id: String
var ad_request: AdRequest
var interstitial_ad_load_callback: InterstitialAdLoadCallback
var _uid: int

func _init():
	if _plugin:
		_uid = _plugin.create()

func load(
	ad_unit_id: String, 
	ad_request: AdRequest, 
	interstitial_ad_load_callback: = InterstitialAdLoadCallback.new()) -> void :

	if _plugin:
		_plugin.load(ad_unit_id, ad_request.convert_to_dictionary(), ad_request.keywords, _uid)
		_plugin.connect("on_interstitial_ad_loaded", func(uid: int):
			if uid == _uid:
				interstitial_ad_load_callback.on_ad_loaded.call_deferred(InterstitialAd.new(uid))
		, CONNECT_ONE_SHOT)
		_plugin.connect("on_interstitial_ad_failed_to_load", func(uid: int, load_ad_error_dictionary: Dictionary):
			if uid == _uid:
				interstitial_ad_load_callback.on_ad_failed_to_load.call_deferred(LoadAdError.create(load_ad_error_dictionary))
		, CONNECT_ONE_SHOT)
