





















class_name UserMessagingPlatform
extends MobileSingletonPlugin

static var _plugin: = _get_plugin("PoingGodotAdMobUserMessagingPlatform")

static var consent_information: = ConsentInformation.new()

static func load_consent_form(
		on_consent_form_load_success_listener: = func(consent_form: ConsentForm): pass, 
		on_consent_form_load_failure_listener: = func(form_error: FormError): pass) -> void :
	if _plugin:
		_plugin.load_consent_form()

		_plugin.connect("on_consent_form_load_success_listener", func(UID: int):
			on_consent_form_load_success_listener.call_deferred(ConsentForm.new(UID))
		, CONNECT_ONE_SHOT)

		_plugin.connect("on_consent_form_load_failure_listener", func(form_error_dictionary: Dictionary):
			on_consent_form_load_failure_listener.call_deferred(FormError.create(form_error_dictionary))
		, CONNECT_ONE_SHOT)
