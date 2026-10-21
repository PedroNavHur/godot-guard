func f(x):
	match x:
		1, 2, 3:
			pass
		[var a, _, ..]:
			pass
		{"key": var v, "other": 2, ..}:
			pass
		var y when y > 10 and y < 20:
			pass
		Constants.VALUE, -1, ~2:
			pass
		_:
			pass
