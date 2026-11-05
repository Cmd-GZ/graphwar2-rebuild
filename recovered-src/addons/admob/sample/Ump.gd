





















extends VBoxContainer

func _ready():
	var request: = ConsentRequestParameters.new()
	var consent_debug_settings: = ConsentDebugSettings.new()
	consent_debug_settings.debug_geography = DebugGeography.Values.EEA
	consent_debug_settings.test_device_hashed_ids.append("test_device_hashed_id")
	request.consent_debug_settings = consent_debug_settings

	UserMessagingPlatform.consent_information.update(request, _on_consent_info_updated_success, _on_consent_info_updated_failure)


func _on_consent_info_updated_failure(form_error: FormError):
	print("_on_consent_info_updated_failure, form_error: error_code=" + str(form_error.error_code) + " message=" + form_error.message)

func _on_consent_info_updated_success():
	print("_on_consent_info_updated_success")
	if UserMessagingPlatform.consent_information.get_is_consent_form_available():
		print("form is available")
		load_form()

func load_form():
	UserMessagingPlatform.load_consent_form(_on_consent_form_load_success, _on_consent_form_load_failure)

func _on_consent_form_load_failure(form_error: FormError):
	print("_on_consent_form_load_failure, form_error: error_code=" + str(form_error.error_code) + " message=" + form_error.message)

func _on_consent_form_load_success(consent_form: ConsentForm):
	if UserMessagingPlatform.consent_information.get_consent_status() == UserMessagingPlatform.consent_information.ConsentStatus.REQUIRED:
		consent_form.show(_on_consent_form_dismissed)

func _on_consent_form_dismissed(form_error: FormError):
	if UserMessagingPlatform.consent_information.get_consent_status() == UserMessagingPlatform.consent_information.ConsentStatus.OBTAINED:
		print("The consent was OBTAINED, you can start request ads")


func _on_reset_consent_information_pressed():
	UserMessagingPlatform.consent_information.reset()

func _on_get_consent_status_pressed():
	print("ConsentStatus: " + ConsentInformation.ConsentStatus.keys()[UserMessagingPlatform.consent_information.get_consent_status()])
