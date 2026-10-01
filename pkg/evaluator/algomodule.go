package evaluator

import (
	"container/heap"
	"fmt"
	"math"
	"math/bits"
	"sort"
	"tiger/pkg/object"
	"unicode"
)

// maxFibonacci is the largest index whose Fibonacci number is exact in a float64.
const maxFibonacci = 78

// maxFactorial is the largest n with n! <= 2^53 - 1, so every result is exact and prints in full.
const maxFactorial = 18

func fibonacciTerms(n int) []object.Value {
	terms := make([]object.Value, n+1)
	previous, current := 0.0, 1.0
	for index := range terms {
		terms[index] = object.Number(previous)
		previous, current = current, previous+current
	}
	return terms
}

func factorialTerms(n int) []object.Value {
	terms := make([]object.Value, n+1)
	product := 1.0
	for index := range terms {
		if index > 0 {
			product *= float64(index)
		}
		terms[index] = object.Number(product)
	}
	return terms
}

func algoModule() *object.Module {
	env := object.NewEnvironment(nil)
	_ = env.Define("fibonacci", sequenceBuiltin("fibonacci", maxFibonacci, false, fibonacciTerms), true)
	_ = env.Define("fibonacciList", sequenceBuiltin("fibonacciList", maxFibonacci, true, fibonacciTerms), true)
	_ = env.Define("factorial", sequenceBuiltin("factorial", maxFactorial, false, factorialTerms), true)
	_ = env.Define("factorialList", sequenceBuiltin("factorialList", maxFactorial, true, factorialTerms), true)
	_ = env.Define("removeDuplicates", listBuiltin("removeDuplicates", 1, 1, func(lists []*object.List, _ []object.Value) (object.Value, error) {
		return &object.List{Elements: uniqueWhere(lists[0], func(object.Value) bool { return true })}, nil
	}), true)
	_ = env.Define("findMatches", listBuiltin("findMatches", 2, 2, func(lists []*object.List, _ []object.Value) (object.Value, error) {
		other := &valueSet{}
		for _, element := range lists[1].Elements {
			other.add(element)
		}
		return &object.List{Elements: uniqueWhere(lists[0], other.contains)}, nil
	}), true)
	_ = env.Define("takeInventory", listBuiltin("takeInventory", 1, 1, takeInventory), true)
	_ = env.Define("findPlace", listBuiltin("findPlace", 2, 1, findPlace), true)
	_ = env.Define("isPrime", &object.Builtin{Name: "algo.isPrime", Call: isPrime}, true)
	_ = env.Define("rainwater", listBuiltin("rainwater", 1, 1, rainwater), true)
	_ = env.Define("medianSorted", listBuiltin("medianSorted", 2, 2, medianSorted), true)
	_ = env.Define("mergeKSorted", listBuiltin("mergeKSorted", 1, 1, mergeKSorted), true)
	_ = env.Define("editDistance", stringPairBuiltin("editDistance", editDistance), true)
	_ = env.Define("regexMatch", stringPairBuiltin("regexMatch", regexMatch), true)
	_ = env.Define("slidingWindowMax", windowBuiltin("slidingWindowMax", slidingWindowMax), true)
	_ = env.Define("slidingWindowSumMax", windowBuiltin("slidingWindowSumMax", slidingWindowSumMax), true)
	_ = env.Define("slidingWindowMinLen", listBuiltin("slidingWindowMinLen", 2, 1, slidingWindowMinLen), true)
	_ = env.Define("slidingWindowLongestUnique", stringsBuiltin("slidingWindowLongestUnique", 1, false, func(words [][]rune) (object.Value, error) {
		return slidingWindowLongestUnique(words[0]), nil
	}), true)
	_ = env.Define("slidingWindowMinSubstring", stringsBuiltin("slidingWindowMinSubstring", 2, false, func(words [][]rune) (object.Value, error) {
		return slidingWindowMinSubstring(words[0], words[1]), nil
	}), true)
	_ = env.Define("palindromeValid", stringsBuiltin("palindromeValid", 1, false, func(words [][]rune) (object.Value, error) {
		return object.Bool(palindromeValid(words[0])), nil
	}), true)
	_ = env.Define("palindromeCanBeValid", stringsBuiltin("palindromeCanBeValid", 1, false, func(words [][]rune) (object.Value, error) {
		return object.Bool(palindromeCanBeValid(words[0])), nil
	}), true)
	_ = env.Define("palindromeLongest", quadraticStringBuiltin("palindromeLongest", func(text []rune) object.Value {
		start, length := palindromeLongest(text)
		return object.String(text[start : start+length])
	}), true)
	_ = env.Define("palindromeCount", quadraticStringBuiltin("palindromeCount", palindromeCount), true)
	_ = env.Define("palindromeMinCuts", quadraticStringBuiltin("palindromeMinCuts", palindromeMinCuts), true)
	return &object.Module{Name: "algo", Env: env}
}

// maxQuadraticLength keeps the O(n^2) palindrome scans within maxDPCells steps.
const maxQuadraticLength = 5000

func quadraticStringBuiltin(name string, function func([]rune) object.Value) *object.Builtin {
	return stringsBuiltin(name, 1, false, func(words [][]rune) (object.Value, error) {
		if len(words[0]) > maxQuadraticLength {
			return nil, fmt.Errorf("input is too long (%d characters; the limit is %d)", len(words[0]), maxQuadraticLength)
		}
		return function(words[0]), nil
	})
}

// palindromeValid compares letters and digits from both ends, ignoring case and everything else.
func palindromeValid(text []rune) bool {
	alphanumeric := func(char rune) bool { return unicode.IsLetter(char) || unicode.IsDigit(char) }
	for left, right := 0, len(text)-1; left < right; left, right = left+1, right-1 {
		for left < right && !alphanumeric(text[left]) {
			left++
		}
		for left < right && !alphanumeric(text[right]) {
			right--
		}
		if unicode.ToLower(text[left]) != unicode.ToLower(text[right]) {
			return false
		}
	}
	return true
}

func isPalindrome(text []rune, left, right int) bool {
	for ; left < right; left, right = left+1, right-1 {
		if text[left] != text[right] {
			return false
		}
	}
	return true
}

// palindromeCanBeValid allows deleting one character at the first mismatch, trying either side.
func palindromeCanBeValid(text []rune) bool {
	for left, right := 0, len(text)-1; left < right; left, right = left+1, right-1 {
		if text[left] != text[right] {
			return isPalindrome(text, left+1, right) || isPalindrome(text, left, right-1)
		}
	}
	return true
}

// expand grows a palindrome outward from text[left..right] and returns its bounds.
func expand(text []rune, left, right int) (int, int) {
	for left >= 0 && right < len(text) && text[left] == text[right] {
		left, right = left-1, right+1
	}
	return left + 1, right - left - 1
}

// palindromeLongest expands around all 2n-1 centers and keeps the first longest palindrome.
func palindromeLongest(text []rune) (int, int) {
	bestStart, bestLength := 0, 0
	for center := 0; center < len(text); center++ {
		for _, even := range []int{0, 1} {
			if start, length := expand(text, center, center+even); length > bestLength {
				bestStart, bestLength = start, length
			}
		}
	}
	return bestStart, bestLength
}

func palindromeCount(text []rune) object.Value {
	count := 0
	for center := 0; center < len(text); center++ {
		for _, even := range []int{0, 1} {
			for left, right := center, center+even; left >= 0 && right < len(text) && text[left] == text[right]; left, right = left-1, right+1 {
				count++
			}
		}
	}
	return object.Number(count)
}

// palindromeMinCuts expands around each center; cuts[i] is the fewest cuts for text[:i], with cuts[0] = -1.
func palindromeMinCuts(text []rune) object.Value {
	cuts := make([]int, len(text)+1)
	for index := range cuts {
		cuts[index] = index - 1
	}
	for center := 0; center < len(text); center++ {
		for _, even := range []int{0, 1} {
			for left, right := center, center+even; left >= 0 && right < len(text) && text[left] == text[right]; left, right = left-1, right+1 {
				cuts[right+1] = min(cuts[right+1], cuts[left]+1)
			}
		}
	}
	return object.Number(max(cuts[len(text)], 0))
}

// maxDPCells bounds the (m+1)*(n+1) dynamic-programming work of the string algorithms.
const maxDPCells = 25_000_000

func stringPairBuiltin(name string, function func(first, second []rune) (object.Value, error)) *object.Builtin {
	return stringsBuiltin(name, 2, true, func(words [][]rune) (object.Value, error) {
		return function(words[0], words[1])
	})
}

// stringsBuiltin checks for count string arguments and passes them as runes; limitCells caps m*n work.
func stringsBuiltin(name string, count int, limitCells bool, function func([][]rune) (object.Value, error)) *object.Builtin {
	qualified := "algo." + name
	return &object.Builtin{Name: qualified, Call: func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, fmt.Errorf("%s does not accept keyword arguments", qualified)
		}
		if len(args) != count {
			return nil, fmt.Errorf("%s expects %d argument%s, got %d", qualified, count, map[bool]string{true: "s"}[count != 1], len(args))
		}
		words := make([][]rune, count)
		cells := 1
		for index, arg := range args {
			text, ok := arg.(object.String)
			if !ok {
				if count == 1 {
					return nil, fmt.Errorf("%s expects a string, got %s", qualified, arg.Type())
				}
				return nil, fmt.Errorf("%s expects two strings, got %s and %s", qualified, args[0].Type(), args[1].Type())
			}
			words[index] = []rune(string(text))
			cells *= len(words[index]) + 1
		}
		if limitCells && cells > maxDPCells {
			return nil, fmt.Errorf("%s: inputs are too long (%d table cells; the limit is %d)", qualified, cells, maxDPCells)
		}
		result, err := function(words)
		if err != nil {
			return nil, fmt.Errorf("%s: %w", qualified, err)
		}
		return result, nil
	}}
}

// numbersOf converts a list to numbers, naming the first non-number.
func numbersOf(list *object.List) ([]float64, error) {
	numbers := make([]float64, len(list.Elements))
	for index, element := range list.Elements {
		number, ok := element.(object.Number)
		if !ok {
			return nil, fmt.Errorf("nums must contain numbers, got %s at index %d", element.Type(), index)
		}
		numbers[index] = float64(number)
	}
	return numbers, nil
}

// windowBuiltin validates (nums, k) with 1 <= k <= len(nums) for fixed-size windows.
func windowBuiltin(name string, function func(numbers []float64, k int) object.Value) *object.Builtin {
	return listBuiltin(name, 2, 1, func(lists []*object.List, rest []object.Value) (object.Value, error) {
		numbers, err := numbersOf(lists[0])
		if err != nil {
			return nil, err
		}
		size, ok := rest[0].(object.Number)
		if !ok {
			return nil, fmt.Errorf("k must be a number, got %s", rest[0].Type())
		}
		if math.Trunc(float64(size)) != float64(size) || size < 1 || float64(size) > float64(len(numbers)) {
			return nil, fmt.Errorf("k must be a whole number from 1 to %d (the length of nums), got %s", len(numbers), object.Format(size))
		}
		return function(numbers, int(size)), nil
	})
}

// slidingWindowMax keeps a deque of indexes whose values decrease, so each window's maximum is at the front.
func slidingWindowMax(numbers []float64, k int) object.Value {
	result := make([]object.Value, 0, len(numbers)-k+1)
	deque := make([]int, 0, k)
	for index, number := range numbers {
		if len(deque) > 0 && deque[0] <= index-k {
			deque = deque[1:]
		}
		for len(deque) > 0 && numbers[deque[len(deque)-1]] <= number {
			deque = deque[:len(deque)-1]
		}
		deque = append(deque, index)
		if index >= k-1 {
			result = append(result, object.Number(numbers[deque[0]]))
		}
	}
	return &object.List{Elements: result}
}

func slidingWindowSumMax(numbers []float64, k int) object.Value {
	sum := 0.0
	for _, number := range numbers[:k] {
		sum += number
	}
	best := sum
	for index := k; index < len(numbers); index++ {
		sum += numbers[index] - numbers[index-k]
		best = math.Max(best, sum)
	}
	return object.Number(best)
}

// slidingWindowMinLen grows the window to reach target, then shrinks it from the left while it still does.
func slidingWindowMinLen(lists []*object.List, rest []object.Value) (object.Value, error) {
	numbers, err := numbersOf(lists[0])
	if err != nil {
		return nil, err
	}
	for index, number := range numbers {
		if number < 0 {
			return nil, fmt.Errorf("nums cannot contain negative numbers, got %s at index %d", object.Format(object.Number(number)), index)
		}
	}
	target, ok := rest[0].(object.Number)
	if !ok {
		return nil, fmt.Errorf("target must be a number, got %s", rest[0].Type())
	}
	if target <= 0 {
		return nil, fmt.Errorf("target must be positive, got %s", object.Format(target))
	}
	best, left, sum := 0, 0, 0.0
	for right, number := range numbers {
		sum += number
		for sum >= float64(target) {
			if length := right - left + 1; best == 0 || length < best {
				best = length
			}
			sum -= numbers[left]
			left++
		}
	}
	return object.Number(best), nil
}

// slidingWindowLongestUnique jumps the left edge past the previous occurrence of a repeated character.
func slidingWindowLongestUnique(text []rune) object.Value {
	last := map[rune]int{}
	best, left := 0, 0
	for right, char := range text {
		if previous, seen := last[char]; seen && previous >= left {
			left = previous + 1
		}
		last[char] = right
		best = max(best, right-left+1)
	}
	return object.Number(best)
}

// slidingWindowMinSubstring returns the shortest, then leftmost, window of text containing every character of target with multiplicity.
func slidingWindowMinSubstring(text, target []rune) object.Value {
	if len(target) == 0 {
		return object.String("")
	}
	need := map[rune]int{}
	for _, char := range target {
		need[char]++
	}
	missing := len(target)
	bestStart, bestLength, left := 0, 0, 0
	for right, char := range text {
		if need[char] > 0 {
			missing--
		}
		need[char]--
		for missing == 0 {
			if length := right - left + 1; bestLength == 0 || length < bestLength {
				bestStart, bestLength = left, length
			}
			need[text[left]]++
			if need[text[left]] > 0 {
				missing++
			}
			left++
		}
	}
	return object.String(text[bestStart : bestStart+bestLength])
}

// editDistance is Levenshtein distance with one DP row sized to the shorter word.
func editDistance(first, second []rune) (object.Value, error) {
	if len(first) < len(second) {
		first, second = second, first
	}
	row := make([]int, len(second)+1)
	for column := range row {
		row[column] = column
	}
	for i := 1; i <= len(first); i++ {
		diagonal := row[0]
		row[0] = i
		for j := 1; j <= len(second); j++ {
			above := row[j]
			if first[i-1] == second[j-1] {
				row[j] = diagonal
			} else {
				row[j] = 1 + min(diagonal, above, row[j-1])
			}
			diagonal = above
		}
	}
	return object.Number(row[len(second)]), nil
}

// regexMatch reports whether pattern (with . and *) matches all of text, using a DP table over suffixes.
func regexMatch(text, pattern []rune) (object.Value, error) {
	for index, char := range pattern {
		if char == '*' && (index == 0 || pattern[index-1] == '*') {
			return nil, fmt.Errorf("* at index %d must follow a character or .", index)
		}
	}
	// matches[i][j] reports whether text[i:] matches pattern[j:].
	matches := make([][]bool, len(text)+1)
	for i := range matches {
		matches[i] = make([]bool, len(pattern)+1)
	}
	matches[len(text)][len(pattern)] = true
	for i := len(text); i >= 0; i-- {
		for j := len(pattern) - 1; j >= 0; j-- {
			first := i < len(text) && (pattern[j] == '.' || pattern[j] == text[i])
			if j+1 < len(pattern) && pattern[j+1] == '*' {
				matches[i][j] = matches[i][j+2] || first && matches[i+1][j]
			} else {
				matches[i][j] = first && matches[i+1][j+1]
			}
		}
	}
	return object.Bool(matches[0][0]), nil
}

// sortedNumbers checks that list holds numbers in ascending order.
func sortedNumbers(label string, list *object.List) ([]float64, error) {
	numbers := make([]float64, len(list.Elements))
	for index, element := range list.Elements {
		number, ok := element.(object.Number)
		if !ok {
			return nil, fmt.Errorf("%s must contain numbers, got %s at index %d", label, element.Type(), index)
		}
		if index > 0 && float64(number) < numbers[index-1] {
			return nil, fmt.Errorf("%s is not sorted: %s at index %d comes after %s", label, object.Format(number), index, object.Format(object.Number(numbers[index-1])))
		}
		numbers[index] = float64(number)
	}
	return numbers, nil
}

// medianSorted binary-searches the partition of the shorter list, taking O(log(min(m, n))) steps.
func medianSorted(lists []*object.List, _ []object.Value) (object.Value, error) {
	first, err := sortedNumbers("first list", lists[0])
	if err != nil {
		return nil, err
	}
	second, err := sortedNumbers("second list", lists[1])
	if err != nil {
		return nil, err
	}
	if len(first) > len(second) {
		first, second = second, first
	}
	m, n := len(first), len(second)
	if m+n == 0 {
		return nil, fmt.Errorf("both lists are empty")
	}
	half := (m + n + 1) / 2
	low, high := 0, m
	for low <= high {
		cut := (low + high) / 2
		other := half - cut
		leftA, rightA := math.Inf(-1), math.Inf(1)
		if cut > 0 {
			leftA = first[cut-1]
		}
		if cut < m {
			rightA = first[cut]
		}
		leftB, rightB := math.Inf(-1), math.Inf(1)
		if other > 0 {
			leftB = second[other-1]
		}
		if other < n {
			rightB = second[other]
		}
		switch {
		case leftA > rightB:
			high = cut - 1
		case leftB > rightA:
			low = cut + 1
		default:
			left := math.Max(leftA, leftB)
			if (m+n)%2 == 1 {
				return object.Number(left), nil
			}
			return object.Number((left + math.Min(rightA, rightB)) / 2), nil
		}
	}
	return nil, fmt.Errorf("lists must be sorted")
}

type mergeItem struct {
	value       object.Value
	list, index int
}

type mergeHeap []mergeItem

func (h mergeHeap) Len() int { return len(h) }
func (h mergeHeap) Less(i, j int) bool {
	if lessValue(h[i].value, h[j].value) {
		return true
	}
	if lessValue(h[j].value, h[i].value) {
		return false
	}
	return h[i].list < h[j].list
}

// lessValue compares two numbers or two strings.
func lessValue(left, right object.Value) bool {
	if number, ok := left.(object.Number); ok {
		return number < right.(object.Number)
	}
	return left.(object.String) < right.(object.String)
}
func (h mergeHeap) Swap(i, j int)  { h[i], h[j] = h[j], h[i] }
func (h *mergeHeap) Push(item any) { *h = append(*h, item.(mergeItem)) }
func (h *mergeHeap) Pop() any {
	old := *h
	item := old[len(old)-1]
	*h = old[:len(old)-1]
	return item
}

// mergeKSorted merges with a min-heap of one cursor per list in O(N log k); ties keep list order.
func mergeKSorted(lists []*object.List, _ []object.Value) (object.Value, error) {
	var sources [][]object.Value
	kind := ""
	total := 0
	for position, element := range lists[0].Elements {
		inner, ok := element.(*object.List)
		if !ok {
			return nil, fmt.Errorf("expects a list of lists, got %s at index %d", element.Type(), position)
		}
		for index, value := range inner.Elements {
			switch value.(type) {
			case object.Number, object.String:
			default:
				return nil, fmt.Errorf("list %d must contain numbers or strings, got %s at index %d", position, value.Type(), index)
			}
			if kind == "" {
				kind = value.Type()
			} else if value.Type() != kind {
				return nil, fmt.Errorf("cannot compare %s and %s (list %d, index %d)", kind, value.Type(), position, index)
			}
			if index > 0 && lessValue(value, inner.Elements[index-1]) {
				return nil, fmt.Errorf("list %d is not sorted: %s at index %d comes after %s", position, object.Format(value), index, object.Format(inner.Elements[index-1]))
			}
		}
		sources = append(sources, append([]object.Value(nil), inner.Elements...))
		total += len(inner.Elements)
	}
	cursors := &mergeHeap{}
	for position, source := range sources {
		if len(source) > 0 {
			*cursors = append(*cursors, mergeItem{source[0], position, 0})
		}
	}
	heap.Init(cursors)
	merged := make([]object.Value, 0, total)
	for cursors.Len() > 0 {
		item := heap.Pop(cursors).(mergeItem)
		merged = append(merged, item.value)
		if next := item.index + 1; next < len(sources[item.list]) {
			heap.Push(cursors, mergeItem{sources[item.list][next], item.list, next})
		}
	}
	return &object.List{Elements: merged}, nil
}

// rainwater solves "trapping rain water" with two pointers in O(n) time and O(1) space.
func rainwater(lists []*object.List, _ []object.Value) (object.Value, error) {
	heights := make([]float64, len(lists[0].Elements))
	for index, element := range lists[0].Elements {
		number, ok := element.(object.Number)
		if !ok {
			return nil, fmt.Errorf("heights must be numbers, got %s at index %d", element.Type(), index)
		}
		if number < 0 {
			return nil, fmt.Errorf("heights cannot be negative, got %s at index %d", object.Format(number), index)
		}
		heights[index] = float64(number)
	}
	if len(heights) == 0 {
		return object.Number(0), nil
	}
	left, right := 0, len(heights)-1
	leftMax, rightMax := heights[left], heights[right]
	total := 0.0
	for left < right {
		if leftMax < rightMax {
			left++
			leftMax = math.Max(leftMax, heights[left])
			total += leftMax - heights[left]
		} else {
			right--
			rightMax = math.Max(rightMax, heights[right])
			total += rightMax - heights[right]
		}
	}
	return object.Number(total), nil
}

// maxExactInteger is 2^53 - 1, the largest integer below which every integer is exact in a float64.
const maxExactInteger = 9007199254740991

func isPrime(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
	const name = "algo.isPrime"
	if len(keywords) != 0 {
		return nil, fmt.Errorf("%s does not accept keyword arguments", name)
	}
	if len(args) != 1 {
		return nil, fmt.Errorf("%s expects 1 argument, got %d", name, len(args))
	}
	number, ok := args[0].(object.Number)
	if !ok {
		return nil, fmt.Errorf("%s expects an integer, got %s", name, args[0].Type())
	}
	if math.Trunc(float64(number)) != float64(number) {
		return nil, fmt.Errorf("%s expects an integer, got %s", name, object.Format(number))
	}
	if math.Abs(float64(number)) > maxExactInteger {
		return nil, fmt.Errorf("%s: %s is too large; the maximum is %d", name, object.Format(number), maxExactInteger)
	}
	return object.Bool(number > 1 && isPrime64(uint64(number))), nil
}

var primeBases = []uint64{2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37}

// isPrime64 is Miller-Rabin with the first twelve prime bases, which is exact below 3.3e24.
func isPrime64(n uint64) bool {
	if n < 2 {
		return false
	}
	for _, base := range primeBases {
		if n%base == 0 {
			return n == base
		}
	}
	d, rounds := n-1, 0
	for d%2 == 0 {
		d /= 2
		rounds++
	}
	for _, base := range primeBases {
		x := powMod(base, d, n)
		if x == 1 || x == n-1 {
			continue
		}
		witness := true
		for round := 1; round < rounds && witness; round++ {
			x = mulMod(x, x, n)
			witness = x != n-1
		}
		if witness {
			return false
		}
	}
	return true
}

func mulMod(a, b, modulus uint64) uint64 {
	high, low := bits.Mul64(a, b)
	_, remainder := bits.Div64(high, low, modulus)
	return remainder
}

func powMod(base, exponent, modulus uint64) uint64 {
	result := uint64(1)
	base %= modulus
	for ; exponent > 0; exponent >>= 1 {
		if exponent&1 == 1 {
			result = mulMod(result, base, modulus)
		}
		base = mulMod(base, base, modulus)
	}
	return result
}

// takeInventory counts each value, keyed in first-occurrence order.
func takeInventory(lists []*object.List, _ []object.Value) (object.Value, error) {
	counts := &object.Dict{}
	for _, element := range lists[0].Elements {
		if number, ok := element.(object.Number); ok {
			element = number + 0
		}
		count, _, err := counts.Get(element)
		if err != nil {
			return nil, err
		}
		if _, missing := count.(object.Null); missing {
			count = object.Number(0)
		}
		if err := counts.Set(element, count.(object.Number)+1); err != nil {
			return nil, err
		}
	}
	return counts, nil
}

// findPlace returns the value ranked at place among distinct values, largest first.
func findPlace(lists []*object.List, rest []object.Value) (object.Value, error) {
	number, ok := rest[0].(object.Number)
	if !ok {
		return nil, fmt.Errorf("place must be a number, got %s", rest[0].Type())
	}
	if number < 1 || math.Trunc(float64(number)) != float64(number) {
		return nil, fmt.Errorf("place must be a positive whole number, got %s", object.Format(number))
	}
	values := uniqueWhere(lists[0], func(object.Value) bool { return true })
	for _, value := range values {
		if _, isNumber := value.(object.Number); !isNumber {
			if _, isString := value.(object.String); !isString {
				return nil, fmt.Errorf("cannot rank %s values; use numbers or strings", value.Type())
			}
		}
		if value.Type() != values[0].Type() {
			return nil, fmt.Errorf("cannot compare %s and %s", values[0].Type(), value.Type())
		}
	}
	if float64(number) > float64(len(values)) {
		return nil, fmt.Errorf("place %s is out of range; the list has %d distinct value%s", object.Format(number), len(values), map[bool]string{true: "s"}[len(values) != 1])
	}
	sort.Slice(values, func(i, j int) bool {
		if left, ok := values[i].(object.Number); ok {
			return left > values[j].(object.Number)
		}
		return values[i].(object.String) > values[j].(object.String)
	})
	return values[int(number)-1], nil
}

// valueSet holds values compared with ==, hashing scalars and scanning unhashable values.
type valueSet struct {
	hashed     map[object.Key]bool
	unhashable []object.Value
}

func (set *valueSet) contains(value object.Value) bool {
	if key, err := object.Hash(value); err == nil {
		return set.hashed[key]
	}
	for _, kept := range set.unhashable {
		if object.Equal(kept, value) {
			return true
		}
	}
	return false
}

func (set *valueSet) add(value object.Value) {
	if set.contains(value) {
		return
	}
	if key, err := object.Hash(value); err == nil {
		if set.hashed == nil {
			set.hashed = map[object.Key]bool{}
		}
		set.hashed[key] = true
		return
	}
	set.unhashable = append(set.unhashable, value)
}

// uniqueWhere returns the first occurrence of each element of list that satisfies keep, in order.
func uniqueWhere(list *object.List, keep func(object.Value) bool) []object.Value {
	seen := &valueSet{}
	result := []object.Value{}
	for _, element := range list.Elements {
		if seen.contains(element) || !keep(element) {
			continue
		}
		seen.add(element)
		result = append(result, element)
	}
	return result
}

// listBuiltin checks arity and that the first listCount arguments are lists.
func listBuiltin(name string, arity, listCount int, function func([]*object.List, []object.Value) (object.Value, error)) *object.Builtin {
	qualified := "algo." + name
	return &object.Builtin{Name: qualified, Call: func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, fmt.Errorf("%s does not accept keyword arguments", qualified)
		}
		if len(args) != arity {
			return nil, fmt.Errorf("%s expects %d argument%s, got %d", qualified, arity, map[bool]string{true: "s"}[arity != 1], len(args))
		}
		lists := make([]*object.List, listCount)
		for index, arg := range args[:listCount] {
			list, ok := arg.(*object.List)
			if !ok {
				return nil, fmt.Errorf("%s expects a list, got %s", qualified, arg.Type())
			}
			lists[index] = list
		}
		result, err := function(lists, args[listCount:])
		if err != nil {
			return nil, fmt.Errorf("%s: %w", qualified, err)
		}
		return result, nil
	}}
}

// sequenceBuiltin validates an index 0..limit and returns either its term or the list of terms 0..n.
func sequenceBuiltin(name string, limit int, list bool, terms func(n int) []object.Value) *object.Builtin {
	qualified := "algo." + name
	return &object.Builtin{Name: qualified, Call: func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, fmt.Errorf("%s does not accept keyword arguments", qualified)
		}
		if len(args) != 1 {
			return nil, fmt.Errorf("%s expects 1 argument, got %d", qualified, len(args))
		}
		index, ok := args[0].(object.Number)
		if !ok {
			return nil, fmt.Errorf("%s expects a number, got %s", qualified, args[0].Type())
		}
		if index < 0 || math.Trunc(float64(index)) != float64(index) {
			return nil, fmt.Errorf("%s expects a non-negative integer, got %s", qualified, object.Format(index))
		}
		if index > object.Number(limit) {
			return nil, fmt.Errorf("%s: index %s is too large; the maximum is %d", qualified, object.Format(index), limit)
		}
		sequence := terms(int(index))
		if list {
			return &object.List{Elements: sequence}, nil
		}
		return sequence[len(sequence)-1], nil
	}}
}
