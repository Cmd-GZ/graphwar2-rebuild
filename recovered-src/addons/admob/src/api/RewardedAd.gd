





















class_name RewardedAd
extends MobileSingletonPlugin

static var _plugin = _get_plugin("PoingGodotAdMobRewardedAd")
var full_screen_content_callback: = FullScreenContentCallback.new()

var _uid: int

func _init(uid: int):
	self._uid = uid
	register_callbacks()

func show(on_user_earned_reward_listener: = OnUserEarnedRewardListener.new()) -> void :
	if _plugin:
		_plugin.show(_uid)
		_plugin.connect("on_rewarded_ad_user_earned_reward", func(uid: int, rewarded_item_dictionary: Dictionary):
			if uid == _uid:
				on_user_earned_reward_listener.on_user_earned_reward.call_deferred(RewardedItem.create(rewarded_item_dictionary))
			)

func destroy() -> void :
	if _plugin:
		_plugin.destroy(_uid)

func set_server_side_verification_options(server_side_verification_options: ServerSideVerificationOptions):
	if _plugin:
		_plugin.set_server_side_verification_options(_uid, server_side_verification_options.convert_to_dictionary())

func register_callbacks() -> void :
	if _plugin:
		_plugin.connect("on_rewarded_ad_clicked", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_clicked.call_deferred()
			)
		_plugin.connect("on_rewarded_ad_dismissed_full_screen_content", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_dismissed_full_screen_content.call_deferred()
			)
		_plugin.connect("on_rewarded_ad_failed_to_show_full_screen_content", func(uid: int, ad_error_dictionary: Dictionary):
			if uid == _uid:
				full_screen_content_callback.on_ad_failed_to_show_full_screen_content.call_deferred(AdError.create(ad_error_dictionary))
			)
		_plugin.connect("on_rewarded_ad_impression", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_impression.call_deferred()
			)
		_plugin.connect("on_rewarded_ad_showed_full_screen_content", func(uid: int):
			if uid == _uid:
				full_screen_content_callback.on_ad_showed_full_screen_content.call_deferred()
			)
