# Blank lines and comments where blocks end, three levels deep: _find_dedent_line_number.
func f(x):
	if x:
		for i in x:
			pass

			# still in the loop
		# back in the if

	# back in the function


# at the top
func g():
	pass
