extends SceneTree


func _process(_delta: float) -> bool:
	var root := get_root().get_node_or_null("Main")
	var probes := [root]
	return false
