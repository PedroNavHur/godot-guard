extends SceneTree

## Loads every GDScript under the given roots so the analyzer reports each warning and
## parse error once; warning severities come from project.godot.
## Run: godot --headless --path . --script analyze_scripts.gd -- --root res://scripts


func _initialize() -> void:
	var roots: Array[String] = []
	var excludes: Array[String] = []
	_read_arguments(roots, excludes)
	var paths: Array[String] = []
	for root_path: String in roots:
		_collect(root_path, excludes, paths)
	paths.sort()
	var failed := 0
	for path: String in paths:
		var script := load(path) as GDScript
		if script == null or not script.can_instantiate():
			failed += 1
			printerr("ANALYZER: failed to load %s" % path)
	print("Analyzed %d scripts, %d failed to load." % [paths.size(), failed])
	quit(1 if failed > 0 else 0)


func _read_arguments(roots: Array[String], excludes: Array[String]) -> void:
	var arguments := OS.get_cmdline_user_args()
	for index: int in range(arguments.size() - 1):
		if arguments[index] == "--root":
			roots.append(arguments[index + 1])
		elif arguments[index] == "--exclude":
			excludes.append("res://" + arguments[index + 1])
	if roots.is_empty():
		roots.append("res://")


func _collect(directory: String, excludes: Array[String], paths: Array[String]) -> void:
	var access := DirAccess.open(directory)
	if access == null or access.list_dir_begin() != OK:
		return
	var entry := access.get_next()
	while entry != "":
		_add_entry(directory.path_join(entry), access.current_is_dir(), excludes, paths)
		entry = access.get_next()
	access.list_dir_end()


func _add_entry(full: String, is_dir: bool, excludes: Array[String], paths: Array[String]) -> void:
	if _is_excluded(full, excludes):
		return
	if is_dir and not full.get_file().begins_with("."):
		_collect(full, excludes, paths)
	elif not is_dir and full.ends_with(".gd"):
		paths.append(full)


func _is_excluded(path: String, excludes: Array[String]) -> bool:
	for pattern: String in excludes:
		if path.match(pattern) or path.begins_with(pattern.trim_suffix("/") + "/"):
			return true
	return false
