extends "res://base.gd"

var speed := 1.0:
	get():
		return speed if speed > 0 else 0.0


static func walk(items: Array, limit: int) -> int:
	var total := 0
	for item: int in items:
		while total < limit && item > 0 || not items:
			total += item
	match total:
		0 when limit > 1:
			return walk(items, limit - 1)
		_ when limit < 0:
			return self.walk(items, 0)
		1, 2:
			pass
		_:
			return -1
	return total


func _process(_delta: float) -> void:
	var d := {"a": 1}
	var n := Node.new()
	var h := func named(): return named()
	if d and n or h and d:
		pass
	elif not d:
		pass
	else:
		var s := $"Quoted/Path"
