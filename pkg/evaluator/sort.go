package evaluator

// stableSort orders items in place with an ascending less function. It is a
// bottom-up merge sort, so it is stable like sort.SliceStable, and it keeps the
// sort, slices, and cmp packages out of the browser Wasm build.
func stableSort[T any](items []T, less func(left, right T) bool) {
	size := len(items)
	if size < 2 {
		return
	}
	// Insertion sort for short slices avoids the merge sort's scratch buffer.
	if size <= 12 {
		for index := 1; index < size; index++ {
			for position := index; position > 0 && less(items[position], items[position-1]); position-- {
				items[position], items[position-1] = items[position-1], items[position]
			}
		}
		return
	}
	scratch := make([]T, size)
	for width := 1; width < size; width *= 2 {
		for start := 0; start < size; start += 2 * width {
			middle := start + width
			if middle > size {
				middle = size
			}
			end := start + 2*width
			if end > size {
				end = size
			}
			mergeRun(items, scratch, start, middle, end, less)
		}
		copy(items, scratch)
	}
}

// mergeRun merges the sorted runs items[start:middle] and items[middle:end]
// into scratch[start:end], taking from the left run on ties to stay stable.
func mergeRun[T any](items, scratch []T, start, middle, end int, less func(left, right T) bool) {
	left, right := start, middle
	for position := start; position < end; position++ {
		switch {
		case left >= middle:
			scratch[position] = items[right]
			right++
		case right >= end:
			scratch[position] = items[left]
			left++
		case less(items[right], items[left]):
			scratch[position] = items[right]
			right++
		default:
			scratch[position] = items[left]
			left++
		}
	}
}
