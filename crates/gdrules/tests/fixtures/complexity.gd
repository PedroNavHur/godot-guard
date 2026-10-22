extends Node

var health: int = 0:
	set(value):
		if value < 0 and health > 0 or value > 100:
			health = 0
		else:
			health = value
	get:
		return health


func chain(x: int) -> String:
	if x == 0:
		return "zero"
	elif x == 1:
		return "one"
	elif x == 2:
		return "two"
	else:
		return "many"


func matcher(x: int) -> int:
	match x:
		1:
			return 1
		2 when x > 0:
			return 2
		_ when x < 0:
			return 3
		_:
			return 4


func with_lambda(items: Array) -> Array:
	return items.filter(func(item): return item > 0 and item < 10 if item else false)


func loops(items: Array) -> int:
	var total := 0
	for item in items:
		while total < item:
			if item % 2 == 0:
				total += 1
	return total


func recurse(n: int) -> int:
	if n <= 0:
		return 0
	return recurse(n - 1) + self.recurse(n - 2)


class Inner:
	func helper(a: bool, b: bool, c: bool) -> bool:
		return a and b and c or not (a or b)
