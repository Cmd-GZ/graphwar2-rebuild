





















class_name InterstitialAd
extends MobileSingletonPlugin

static var _plugin = _get_plugin("PoingGodotAdMobInterstitialAd")
var full_screen_content_callback: = FullScreenContentCallback.new()

var _uid: int

func _init(uid: int):
	self._uid = uid
	register_callbacks()

func show() -> void :
	if _plugin:
		_plugin.show(_uid)

func destroy() -> void :
	if _plugin:
		_plugin.destroy(_uid)

func register_callbacks() -> void :
	if _plugin:
		_plugin.connect("on_interstitial_ad_clicked", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_clicked.call_deferred()
			)
		_plugin.connect("on_interstitial_ad_dismissed_full_screen_content", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_dismissed_full_screen_content.call_deferred()
			)
		_plugin.connect("on_interstitial_ad_failed_to_show_full_screen_content", func(uid: int, ad_error_dictionary: Dictionary):
			if uid == _uid:
				full_screen_content_callback.on_ad_failed_to_show_full_screen_content.call_deferred(AdError.create(ad_error_dictionary))
			)
		_plugin.connect("on_interstitial_ad_impression", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_impression.call_deferred()
			)
		_plugin.connect("on_interstitial_ad_showed_full_screen_content", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_showed_full_screen_content.call_deferred()
			)
