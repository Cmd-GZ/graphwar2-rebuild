extends Control

var album_ios = null
var root = null


func _ready():
	root = get_node("/root/GraphwarRootNode")

	if Engine.has_singleton("AlbumIOS"):
		root.log("AlbumIOS found")
		album_ios = Engine.get_singleton("AlbumIOS")

func publish_image_to_album(image_path: String):
	root.log("publishing image: " + image_path)
	if album_ios != null:
		album_ios.publish_image_to_album(image_path)

func publish_video_to_album(video_path: String):
	root.log("publishing video: " + video_path)
	if album_ios != null:
		album_ios.publish_video_to_album(video_path)
