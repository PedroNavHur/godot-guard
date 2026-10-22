extends Node


func pick(a: bool, b: bool) -> int:
	var flat := 1 if a else 2
	var chained := 1 if a else 2 if b else 3
	var wrapped := (1 if a else 2) if b else 3
	var in_condition := 1 if (a if b else false) else 2
	return flat + chained + wrapped + in_condition
