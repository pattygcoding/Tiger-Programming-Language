package evaluator

import (
	"bytes"
	"math"
	"math/big"
	"math/rand"
	"regexp"
	"sort"
	"strings"
	"testing"
	"tiger/pkg/object"
	"unicode"
)

func TestAlgoModule(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"fibonacci", `var target = algo.fibonacci(6); print(target);`, "8\n"},
		{"fibonacci list", `var sequence = algo.fibonacciList(6); print(sequence);`, "[0, 1, 1, 2, 3, 5, 8]\n"},
		{"small indexes", `print(algo.fibonacci(0), algo.fibonacci(1), algo.fibonacci(2), algo.fibonacci(10), algo.fibonacci(6.0));`, "0 1 1 55 8\n"},
		{"small lists", `print(algo.fibonacciList(0), algo.fibonacciList(1), len(algo.fibonacciList(20)));`, "[0] [0, 1] 21\n"},
		{"largest exact", `print(algo.fibonacci(78), algo.fibonacciList(78)[78]);`, "8944394323791464 8944394323791464\n"},
		{"list is fresh and mutable", `var a = algo.fibonacciList(3); a += [99]; a[0] = -1; print(a, algo.fibonacciList(3));`, "[-1, 1, 1, 2, 99] [0, 1, 1, 2]\n"},
		{"module value", `print(algo, algo.fibonacci);`, "<module algo> <builtin algo.fibonacci>\n"},
		{"shadowed", `const algo = 1; print(algo);`, "1\n"},
		{"factorial", `print(algo.factorial(0), algo.factorial(1), algo.factorial(5), algo.factorial(10), algo.factorial(6.0), algo.factorial(18));`, "1 1 120 3628800 720 6402373705728000\n"},
		{"factorial list", `var a = algo.factorialList(5); a[0] = 99; print(algo.factorialList(0), a, algo.factorialList(5), len(algo.factorialList(18)));`, "[1] [99, 1, 2, 6, 24, 120] [1, 1, 2, 6, 24, 120] 19\n"},
		{"palindrome valid", `print(algo.palindromeValid("A man, a plan, a canal: Panama"), algo.palindromeValid("race a car"), algo.palindromeValid(" "), algo.palindromeValid(""), algo.palindromeValid("0P"), algo.palindromeValid("Été été"), algo.palindromeValid("No 'x' in Nixon"));`, "true false true true false true true\n"},
		{"palindrome can be valid", `print(algo.palindromeCanBeValid("aba"), algo.palindromeCanBeValid("abca"), algo.palindromeCanBeValid("abc"), algo.palindromeCanBeValid(""), algo.palindromeCanBeValid("deeee"), algo.palindromeCanBeValid("cbbcc"), algo.palindromeCanBeValid("Aba"));`, "true true false true true true false\n"},
		{"palindrome longest", `print(algo.palindromeLongest("babad"), algo.palindromeLongest("cbbd"), algo.palindromeLongest("a"), algo.palindromeLongest("") == "", algo.palindromeLongest("forgeeksskeegfor"), algo.palindromeLongest("abc"));`, "bab bb a true geeksskeeg a\n"},
		{"palindrome count", `print(algo.palindromeCount("abc"), algo.palindromeCount("aaa"), algo.palindromeCount(""), algo.palindromeCount("abba"));`, "3 6 0 6\n"},
		{"palindrome min cuts", `print(algo.palindromeMinCuts("aab"), algo.palindromeMinCuts("a"), algo.palindromeMinCuts("ab"), algo.palindromeMinCuts(""), algo.palindromeMinCuts("racecar"), algo.palindromeMinCuts("abcde"), algo.palindromeMinCuts("banana"));`, "1 0 1 0 0 4 1\n"},
		{"sliding window max", `print(algo.slidingWindowMax([1, 3, -1, -3, 5, 3, 6, 7], 3), algo.slidingWindowMax([1], 1), algo.slidingWindowMax([4, 2], 2), algo.slidingWindowMax([2, 2, 2], 1));`, "[3, 3, 5, 5, 6, 7] [1] [4] [2, 2, 2]\n"},
		{"sliding window sum max", `print(algo.slidingWindowSumMax([1, 12, -5, -6, 50, 3], 4), algo.slidingWindowSumMax([5], 1), algo.slidingWindowSumMax([-1, -2, -3], 2), algo.slidingWindowSumMax([0.5, 1.5, 2], 3));`, "51 5 -3 4\n"},
		{"sliding window min len", `print(algo.slidingWindowMinLen([2, 3, 1, 2, 4, 3], 7), algo.slidingWindowMinLen([1, 4, 4], 4), algo.slidingWindowMinLen([1, 1, 1, 1, 1, 1, 1, 1], 11), algo.slidingWindowMinLen([], 3), algo.slidingWindowMinLen([0, 0, 5], 5), algo.slidingWindowMinLen([1, 2, 3], 6));`, "2 1 0 0 1 3\n"},
		{"sliding window longest unique", `print(algo.slidingWindowLongestUnique("abcabcbb"), algo.slidingWindowLongestUnique("bbbbb"), algo.slidingWindowLongestUnique("pwwkew"), algo.slidingWindowLongestUnique(""), algo.slidingWindowLongestUnique("abba"), algo.slidingWindowLongestUnique("café café"));`, "3 1 3 0 2 5\n"},
		{"sliding window min substring", `print(algo.slidingWindowMinSubstring("ADOBECODEBANC", "ABC"), algo.slidingWindowMinSubstring("a", "a"), algo.slidingWindowMinSubstring("a", "aa") == "", algo.slidingWindowMinSubstring("aa", "aa"), algo.slidingWindowMinSubstring("abc", "") == "", algo.slidingWindowMinSubstring("xabyab", "ab"));`, "BANC a true aa true ab\n"},
		{"edit distance", `print(algo.editDistance("horse", "ros"), algo.editDistance("intention", "execution"), algo.editDistance("", "abc"), algo.editDistance("abc", ""), algo.editDistance("same", "same"), algo.editDistance("kitten", "sitting"), algo.editDistance("ros", "horse"), algo.editDistance("café", "cafe"), algo.editDistance("Tiger", "tiger"));`, "3 5 3 3 0 3 3 1 1\n"},
		{"regex match", `print(algo.regexMatch("aa", "a*"), algo.regexMatch("aa", "a"), algo.regexMatch("ab", ".*"), algo.regexMatch("aab", "c*a*b"), algo.regexMatch("mississippi", "mis*is*p*."), algo.regexMatch("", "a*b*"), algo.regexMatch("", ""), algo.regexMatch("a", ""), algo.regexMatch("ab", ".*c"), algo.regexMatch("aaa", "ab*a*c*a"), algo.regexMatch("é", "."), algo.regexMatch("a+b", "a+b"));`, "true false true true false true true false false true true true\n"},
		{"median sorted", `print(algo.medianSorted([1, 3], [2]), algo.medianSorted([1, 2], [3, 4]));`, "2 2.5\n"},
		{"median sorted shapes", `print(algo.medianSorted([], [1]), algo.medianSorted([2], []), algo.medianSorted([], [1, 2, 3, 4]), algo.medianSorted([1, 1], [1, 1]), algo.medianSorted([5, 6, 7], [1, 2]), algo.medianSorted([-5, 3], [-1, 0, 8, 9]), algo.medianSorted([1.5], [2.5]));`, "1 2 2.5 1 5 1.5 2\n"},
		{"merge k sorted", `const lists = [[1, 4, 5], [1, 3, 4], [2, 6]]; print(algo.mergeKSorted(lists), lists);`, "[1, 1, 2, 3, 4, 4, 5, 6] [[1, 4, 5], [1, 3, 4], [2, 6]]\n"},
		{"merge k sorted shapes", `print(algo.mergeKSorted([]), algo.mergeKSorted([[], []]), algo.mergeKSorted([[3]]), algo.mergeKSorted([["b", "d"], [], ["a", "c", "e"]]), algo.mergeKSorted([[-1.5, 2], [-3, 2, 2]]));`, "[] [] [3] [\"a\", \"b\", \"c\", \"d\", \"e\"] [-3, -1.5, 2, 2, 2]\n"},
		{"rainwater", `var map = [0, 1, 0, 2, 1, 0, 1, 3, 2, 1, 2, 1]; print(algo.rainwater(map), map);`, "6 [0, 1, 0, 2, 1, 0, 1, 3, 2, 1, 2, 1]\n"},
		{"rainwater shapes", `print(algo.rainwater([4, 2, 0, 3, 2, 5]), algo.rainwater([]), algo.rainwater([5]), algo.rainwater([1, 2, 3]), algo.rainwater([3, 2, 1]), algo.rainwater([3, 0, 3]), algo.rainwater([2, 2, 2]));`, "9 0 0 0 0 3 0\n"},
		{"rainwater fractions", `print(algo.rainwater([1.5, 0, 2.5]), algo.rainwater([5, 0, 0, 0, 1]));`, "1.5 3\n"},
		{"is prime small", `var primes = []; for n in range(-3, 30) { if algo.isPrime(n) { primes += [n]; } } print(primes);`, "[2, 3, 5, 7, 11, 13, 17, 19, 23, 29]\n"},
		{"is prime edge cases", `print(algo.isPrime(0), algo.isPrime(1), algo.isPrime(-7), algo.isPrime(2), algo.isPrime(4), algo.isPrime(7.0));`, "false false false true false true\n"},
		{"is prime large", `print(algo.isPrime(7919), algo.isPrime(561), algo.isPrime(1000003), algo.isPrime(999999999989), algo.isPrime(9007199254740881), algo.isPrime(9007199254740991));`, "true false true true true false\n"},
		{"take inventory", `print(algo.takeInventory(["a", "b", "a", "c", "a", "b"]));`, "{\"a\": 3, \"b\": 2, \"c\": 1}\n"},
		{"take inventory mixed keys", `const counts = algo.takeInventory([1, 1.0, "1", true, true, -0, 0]); print(counts, counts[1], counts[true]);`, "{1: 2, \"1\": 1, true: 2, 0: 2} 2 2\n"},
		{"take inventory empty and fresh", `const items = [5]; var counts = algo.takeInventory(items); counts[5] = 9; print(algo.takeInventory([]), algo.takeInventory(items), items);`, "{} {5: 1} [5]\n"},
		{"find place numbers", `const scores = [70, 95, 88, 95, 60, 88]; print(algo.findPlace(scores, 1), algo.findPlace(scores, 2), algo.findPlace(scores, 3), algo.findPlace(scores, 4), scores);`, "95 88 70 60 [70, 95, 88, 95, 60, 88]\n"},
		{"find place strings and floats", `print(algo.findPlace(["pear", "apple", "zebra"], 1), algo.findPlace([1.5, -2, 1.25], 3), algo.findPlace([4, 4.0], 1), algo.findPlace([7], 1.0));`, "zebra -2 4 7\n"},
		{"remove duplicates", `print(algo.removeDuplicates([3, 1, 3, 2, 1, 3]));`, "[3, 1, 2]\n"},
		{"remove duplicates keeps original", `const items = [1, 1, 2]; var unique = algo.removeDuplicates(items); unique += [9]; print(items, unique);`, "[1, 1, 2] [1, 2, 9]\n"},
		{"remove duplicates mixed types", `print(algo.removeDuplicates([1, "1", true, 1.0, "1", false, null, true, null, 0, -0]));`, "[1, \"1\", true, false, null, 0]\n"},
		{"remove duplicates nested", `print(algo.removeDuplicates([[1, 2], [1, 2], [2, 1], {"a": 1}, {"a": 1}, [], []]));`, "[[1, 2], [2, 1], {\"a\": 1}, []]\n"},
		{"remove duplicates instances by identity", `class P {} const p = P(); print(len(algo.removeDuplicates([p, P(), p])));`, "2\n"},
		{"remove duplicates empty", `print(algo.removeDuplicates([]));`, "[]\n"},
		{"find matches", `print(algo.findMatches([1, 2, 3, 4], [4, 3, 9]));`, "[3, 4]\n"},
		{"find matches dedupes in first order", `print(algo.findMatches([5, 1, 5, 2, 1], [1, 1, 5, 5]));`, "[5, 1]\n"},
		{"find matches none or empty", `print(algo.findMatches([1, 2], [3]), algo.findMatches([], [1]), algo.findMatches([1], []));`, "[] [] []\n"},
		{"find matches equality", `print(algo.findMatches([1, "1", true, null, [1, 2], {"a": 1}], [1.0, false, null, [1, 2], {"a": 1}]));`, "[1, null, [1, 2], {\"a\": 1}]\n"},
		{"find matches same list", `const items = [2, 2, 1]; print(algo.findMatches(items, items), items);`, "[2, 1] [2, 2, 1]\n"},
		{"find matches instances by identity", `class P {} const p = P(); print(len(algo.findMatches([p, P()], [p, P()])));`, "1\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
}

func TestAlgoModuleErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`algo.fibonacci(-1);`, "algo.fibonacci expects a non-negative integer, got -1"},
		{`algo.fibonacciList(2.5);`, "algo.fibonacciList expects a non-negative integer, got 2.5"},
		{`algo.fibonacci(79);`, "algo.fibonacci: index 79 is too large; the maximum is 78"},
		{`algo.fibonacciList(1e300);`, "algo.fibonacciList: index"},
		{`algo.fibonacci("6");`, "algo.fibonacci expects a number, got string"},
		{`algo.fibonacci();`, "algo.fibonacci expects 1 argument, got 0"},
		{`algo.fibonacci(1, 2);`, "algo.fibonacci expects 1 argument, got 2"},
		{`algo.fibonacci(n=3);`, "algo.fibonacci does not accept keyword arguments"},
		{`algo.sort;`, `module "algo" has no export "sort"`},
		{`algo.fibonacci = 1;`, "cannot assign a property of module"},
		{`algo.removeDuplicates("aab");`, "algo.removeDuplicates expects a list, got string"},
		{`algo.removeDuplicates({"a": 1});`, "algo.removeDuplicates expects a list, got dict"},
		{`algo.removeDuplicates([1], [2]);`, "algo.removeDuplicates expects 1 argument, got 2"},
		{`algo.removeDuplicates(items=[1]);`, "algo.removeDuplicates does not accept keyword arguments"},
		{`algo.findMatches([1]);`, "algo.findMatches expects 2 arguments, got 1"},
		{`algo.findMatches([1], [2], [3]);`, "algo.findMatches expects 2 arguments, got 3"},
		{`algo.findMatches([1], "1");`, "algo.findMatches expects a list, got string"},
		{`algo.findMatches(null, [1]);`, "algo.findMatches expects a list, got null"},
		{`algo.findMatches(X=[1], Y=[1]);`, "algo.findMatches does not accept keyword arguments"},
		{`algo.takeInventory([1, [2]]);`, "algo.takeInventory: list cannot be a dictionary key"},
		{`algo.takeInventory("abc");`, "algo.takeInventory expects a list, got string"},
		{`algo.takeInventory();`, "algo.takeInventory expects 1 argument, got 0"},
		{`algo.findPlace([3, 1, 2], 4);`, "algo.findPlace: place 4 is out of range; the list has 3 distinct values"},
		{`algo.findPlace([2, 2], 2);`, "algo.findPlace: place 2 is out of range; the list has 1 distinct value"},
		{`algo.findPlace([], 1);`, "algo.findPlace: place 1 is out of range; the list has 0 distinct values"},
		{`algo.findPlace([1, 2], 0);`, "algo.findPlace: place must be a positive whole number, got 0"},
		{`algo.findPlace([1, 2], -1);`, "algo.findPlace: place must be a positive whole number, got -1"},
		{`algo.findPlace([1, 2], 1.5);`, "algo.findPlace: place must be a positive whole number, got 1.5"},
		{`algo.findPlace([1, 2], "1");`, "algo.findPlace: place must be a number, got string"},
		{`algo.findPlace([1, "2"], 1);`, "algo.findPlace: cannot compare number and string"},
		{`algo.findPlace([[1], [2]], 1);`, "algo.findPlace: cannot rank list values; use numbers or strings"},
		{`algo.findPlace(3, 1);`, "algo.findPlace expects a list, got number"},
		{`algo.findPlace([1]);`, "algo.findPlace expects 2 arguments, got 1"},
		{`algo.isPrime(2.5);`, "algo.isPrime expects an integer, got 2.5"},
		{`algo.isPrime("7");`, "algo.isPrime expects an integer, got string"},
		{`algo.isPrime(true);`, "algo.isPrime expects an integer, got bool"},
		{`algo.isPrime(9007199254740992);`, "algo.isPrime: 9007199254740992 is too large; the maximum is 9007199254740991"},
		{`algo.isPrime(-9007199254740992);`, "algo.isPrime: -9007199254740992 is too large"},
		{`algo.isPrime();`, "algo.isPrime expects 1 argument, got 0"},
		{`algo.isPrime(2, 3);`, "algo.isPrime expects 1 argument, got 2"},
		{`algo.isPrime(x=3);`, "algo.isPrime does not accept keyword arguments"},
		{`algo.rainwater([1, "2", 3]);`, "algo.rainwater: heights must be numbers, got string at index 1"},
		{`algo.rainwater([1, -2, 3]);`, "algo.rainwater: heights cannot be negative, got -2 at index 1"},
		{`algo.rainwater("121");`, "algo.rainwater expects a list, got string"},
		{`algo.rainwater();`, "algo.rainwater expects 1 argument, got 0"},
		{`algo.medianSorted([], []);`, "algo.medianSorted: both lists are empty"},
		{`algo.medianSorted([3, 1], [2]);`, "algo.medianSorted: first list is not sorted: 1 at index 1 comes after 3"},
		{`algo.medianSorted([1], [2, "3"]);`, "algo.medianSorted: second list must contain numbers, got string at index 1"},
		{`algo.medianSorted([1]);`, "algo.medianSorted expects 2 arguments, got 1"},
		{`algo.medianSorted([1], 2);`, "algo.medianSorted expects a list, got number"},
		{`algo.mergeKSorted([[1, 2], 3]);`, "algo.mergeKSorted: expects a list of lists, got number at index 1"},
		{`algo.mergeKSorted([[1], [5, 4]]);`, "algo.mergeKSorted: list 1 is not sorted: 4 at index 1 comes after 5"},
		{`algo.mergeKSorted([[1], ["a"]]);`, "algo.mergeKSorted: cannot compare number and string (list 1, index 0)"},
		{`algo.mergeKSorted([[true]]);`, "algo.mergeKSorted: list 0 must contain numbers or strings, got bool at index 0"},
		{`algo.mergeKSorted("abc");`, "algo.mergeKSorted expects a list, got string"},
		{`algo.editDistance("a", 1);`, "algo.editDistance expects two strings, got string and number"},
		{`algo.editDistance("a");`, "algo.editDistance expects 2 arguments, got 1"},
		{`algo.editDistance(word1="a", word2="b");`, "algo.editDistance does not accept keyword arguments"},
		{`algo.regexMatch("a", "*a");`, "algo.regexMatch: * at index 0 must follow a character or ."},
		{`algo.regexMatch("a", "a**");`, "algo.regexMatch: * at index 2 must follow a character or ."},
		{`algo.regexMatch(["a"], "a");`, "algo.regexMatch expects two strings, got list and string"},
		{`algo.slidingWindowMax([1, 2], 3);`, "algo.slidingWindowMax: k must be a whole number from 1 to 2 (the length of nums), got 3"},
		{`algo.slidingWindowMax([1, 2], 0);`, "k must be a whole number from 1 to 2"},
		{`algo.slidingWindowMax([1, 2], 1.5);`, "got 1.5"},
		{`algo.slidingWindowMax([], 1);`, "k must be a whole number from 1 to 0"},
		{`algo.slidingWindowMax([1, 2], "1");`, "algo.slidingWindowMax: k must be a number, got string"},
		{`algo.slidingWindowSumMax([1, "2"], 1);`, "algo.slidingWindowSumMax: nums must contain numbers, got string at index 1"},
		{`algo.slidingWindowSumMax("12", 1);`, "algo.slidingWindowSumMax expects a list, got string"},
		{`algo.slidingWindowMinLen([1, -1], 1);`, "algo.slidingWindowMinLen: nums cannot contain negative numbers, got -1 at index 1"},
		{`algo.slidingWindowMinLen([1], 0);`, "algo.slidingWindowMinLen: target must be positive, got 0"},
		{`algo.slidingWindowMinLen([1], "1");`, "target must be a number, got string"},
		{`algo.slidingWindowMinLen([1]);`, "algo.slidingWindowMinLen expects 2 arguments, got 1"},
		{`algo.slidingWindowLongestUnique(12);`, "algo.slidingWindowLongestUnique expects a string, got number"},
		{`algo.slidingWindowLongestUnique("a", "b");`, "algo.slidingWindowLongestUnique expects 1 argument, got 2"},
		{`algo.slidingWindowMinSubstring("abc", ["a"]);`, "algo.slidingWindowMinSubstring expects two strings, got string and list"},
		{`algo.palindromeValid(121);`, "algo.palindromeValid expects a string, got number"},
		{`algo.factorial(19);`, "algo.factorial: index 19 is too large; the maximum is 18"},
		{`algo.factorialList(-1);`, "algo.factorialList expects a non-negative integer, got -1"},
		{`algo.factorial(2.5);`, "algo.factorial expects a non-negative integer, got 2.5"},
		{`algo.factorial("5");`, "algo.factorial expects a number, got string"},
		{`algo.factorialList();`, "algo.factorialList expects 1 argument, got 0"},
		{`algo.factorial(n=3);`, "algo.factorial does not accept keyword arguments"},
		{`algo.palindromeCanBeValid();`, "algo.palindromeCanBeValid expects 1 argument, got 0"},
		{`algo.palindromeLongest("a", "b");`, "algo.palindromeLongest expects 1 argument, got 2"},
		{`algo.palindromeCount(["a"]);`, "algo.palindromeCount expects a string, got list"},
		{`algo.palindromeMinCuts(s="a");`, "algo.palindromeMinCuts does not accept keyword arguments"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}

func TestMedianAndMergeAgainstSorting(t *testing.T) {
	random := rand.New(rand.NewSource(4))
	list := func(size int) (*object.List, []float64) {
		values := make([]float64, size)
		for index := range values {
			values[index] = float64(random.Intn(20) - 5)
		}
		sort.Float64s(values)
		return numberList(values), values
	}
	for trial := 0; trial < 2000; trial++ {
		first, a := list(random.Intn(8))
		second, b := list(random.Intn(8))
		third, c := list(random.Intn(8))
		all := append(append(append([]float64{}, a...), b...), c...)
		sort.Float64s(all)
		merged, err := mergeKSorted([]*object.List{{Elements: []object.Value{first, second, third}}}, nil)
		if err != nil || object.Format(merged) != object.Format(numberList(all)) {
			t.Fatalf("mergeKSorted(%v, %v, %v) = %v, %v", a, b, c, object.Format(merged), err)
		}
		combined := append(append([]float64{}, a...), b...)
		if len(combined) == 0 {
			continue
		}
		sort.Float64s(combined)
		middle := len(combined) / 2
		want := combined[middle]
		if len(combined)%2 == 0 {
			want = (combined[middle-1] + combined[middle]) / 2
		}
		got, err := medianSorted([]*object.List{first, second}, nil)
		if err != nil || got != object.Number(want) {
			t.Fatalf("medianSorted(%v, %v) = %v, %v; want %v", a, b, got, err, want)
		}
	}
}

func numberList(values []float64) *object.List {
	elements := make([]object.Value, len(values))
	for index, value := range values {
		elements[index] = object.Number(value)
	}
	return &object.List{Elements: elements}
}

func TestStringAlgorithmLimits(t *testing.T) {
	long := strings.Repeat("a", 5001)
	for _, name := range []string{"editDistance", "regexMatch"} {
		err := Run(`algo.`+name+`("`+long+`", "`+long+`");`, nil)
		if err == nil || !strings.Contains(err.Error(), "algo."+name+": inputs are too long (25020004 table cells; the limit is 25000000)") {
			t.Errorf("%s: got %v", name, err)
		}
	}
	var output bytes.Buffer
	if err := Run(`print(algo.editDistance("`+long[:3000]+`", "`+long[:2990]+`b"), algo.regexMatch("`+long[:3000]+`", "a*a*a*"));`, &output); err != nil || output.String() != "10 true\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}

func TestRegexMatchAgainstRegexp(t *testing.T) {
	random := rand.New(rand.NewSource(10))
	word := func(alphabet string, size int) string {
		var builder strings.Builder
		for range size {
			builder.WriteByte(alphabet[random.Intn(len(alphabet))])
		}
		return builder.String()
	}
	for trial := 0; trial < 5000; trial++ {
		text := word("ab", random.Intn(7))
		var pattern strings.Builder
		for range random.Intn(6) {
			pattern.WriteString(word("ab.", 1))
			if random.Intn(2) == 0 {
				pattern.WriteByte('*')
			}
		}
		want := regexp.MustCompile("^(?:" + pattern.String() + ")$").MatchString(text)
		got, err := regexMatch([]rune(text), []rune(pattern.String()))
		if err != nil || got != object.Bool(want) {
			t.Fatalf("regexMatch(%q, %q) = %v, %v; want %v", text, pattern.String(), got, err, want)
		}
	}
}

func TestSlidingWindowsAgainstBruteForce(t *testing.T) {
	random := rand.New(rand.NewSource(7))
	for trial := 0; trial < 3000; trial++ {
		numbers := make([]float64, 1+random.Intn(10))
		for index := range numbers {
			numbers[index] = float64(random.Intn(9))
		}
		k := 1 + random.Intn(len(numbers))
		wantMax := []string{}
		wantSum := math.Inf(-1)
		for start := 0; start+k <= len(numbers); start++ {
			high, sum := numbers[start], 0.0
			for _, number := range numbers[start : start+k] {
				high, sum = math.Max(high, number), sum+number
			}
			wantMax = append(wantMax, object.Format(object.Number(high)))
			wantSum = math.Max(wantSum, sum)
		}
		if got := object.Format(slidingWindowMax(numbers, k)); got != "["+strings.Join(wantMax, ", ")+"]" {
			t.Fatalf("slidingWindowMax(%v, %d) = %s, want %v", numbers, k, got, wantMax)
		}
		if got := slidingWindowSumMax(numbers, k); got != object.Number(wantSum) {
			t.Fatalf("slidingWindowSumMax(%v, %d) = %v, want %v", numbers, k, got, wantSum)
		}
		target := float64(1 + random.Intn(30))
		wantLen := 0
		for start := range numbers {
			sum := 0.0
			for end := start; end < len(numbers); end++ {
				if sum += numbers[end]; sum >= target {
					if wantLen == 0 || end-start+1 < wantLen {
						wantLen = end - start + 1
					}
					break
				}
			}
		}
		if got, err := slidingWindowMinLen([]*object.List{numberList(numbers)}, []object.Value{object.Number(target)}); err != nil || got != object.Number(wantLen) {
			t.Fatalf("slidingWindowMinLen(%v, %v) = %v, %v; want %d", numbers, target, got, err, wantLen)
		}
		text := make([]rune, random.Intn(12))
		for index := range text {
			text[index] = rune('a' + random.Intn(4))
		}
		pattern := make([]rune, 1+random.Intn(3))
		for index := range pattern {
			pattern[index] = rune('a' + random.Intn(4))
		}
		wantUnique, wantWindow := 0, ""
		for start := range text {
			for end := start; end < len(text); end++ {
				window := text[start : end+1]
				seen := map[rune]bool{}
				unique := true
				counts := map[rune]int{}
				for _, char := range window {
					unique = unique && !seen[char]
					seen[char] = true
					counts[char]++
				}
				if unique {
					wantUnique = max(wantUnique, len(window))
				}
				covers := true
				for _, char := range pattern {
					counts[char]--
					covers = covers && counts[char] >= 0
				}
				if covers && (wantWindow == "" || len(window) < len([]rune(wantWindow))) {
					wantWindow = string(window)
				}
			}
		}
		if got := slidingWindowLongestUnique(text); got != object.Number(wantUnique) {
			t.Fatalf("slidingWindowLongestUnique(%q) = %v, want %d", string(text), got, wantUnique)
		}
		if got := slidingWindowMinSubstring(text, pattern); got != object.String(wantWindow) {
			t.Fatalf("slidingWindowMinSubstring(%q, %q) = %q, want %q", string(text), string(pattern), got, wantWindow)
		}
	}
}

func TestPalindromesAgainstBruteForce(t *testing.T) {
	random := rand.New(rand.NewSource(12))
	palindrome := func(text []rune) bool { return isPalindrome(text, 0, len(text)-1) }
	for trial := 0; trial < 3000; trial++ {
		text := make([]rune, random.Intn(12))
		for index := range text {
			text[index] = rune("abA, "[random.Intn(5)])
		}
		var letters []rune
		for _, char := range text {
			if unicode.IsLetter(char) {
				letters = append(letters, unicode.ToLower(char))
			}
		}
		if palindromeValid(text) != palindrome(letters) {
			t.Fatalf("palindromeValid(%q) = %v", string(text), !palindrome(letters))
		}
		canBe := palindrome(text)
		for skip := range text {
			canBe = canBe || palindrome(append(append([]rune{}, text[:skip]...), text[skip+1:]...))
		}
		if palindromeCanBeValid(text) != canBe {
			t.Fatalf("palindromeCanBeValid(%q) = %v", string(text), !canBe)
		}
		count, longest := 0, ""
		for start := range text {
			for end := start + 1; end <= len(text); end++ {
				if palindrome(text[start:end]) {
					count++
					if end-start > len([]rune(longest)) {
						longest = string(text[start:end])
					}
				}
			}
		}
		if got := palindromeCount(text); got != object.Number(count) {
			t.Fatalf("palindromeCount(%q) = %v, want %d", string(text), got, count)
		}
		if start, length := palindromeLongest(text); string(text[start:start+length]) != longest {
			t.Fatalf("palindromeLongest(%q) = %q, want %q", string(text), string(text[start:start+length]), longest)
		}
		cuts := make([]int, len(text)+1)
		for end := 1; end <= len(text); end++ {
			cuts[end] = end
			for start := 0; start < end; start++ {
				if palindrome(text[start:end]) {
					cuts[end] = min(cuts[end], cuts[start]+1)
				}
			}
		}
		want := max(cuts[len(text)]-1, 0)
		if got := palindromeMinCuts(text); got != object.Number(want) {
			t.Fatalf("palindromeMinCuts(%q) = %v, want %d", string(text), got, want)
		}
	}
	long := strings.Repeat("a", maxQuadraticLength+1)
	if err := Run(`algo.palindromeCount("`+long+`");`, nil); err == nil || !strings.Contains(err.Error(), "algo.palindromeCount: input is too long (5001 characters; the limit is 5000)") {
		t.Fatalf("got %v", err)
	}
	var output bytes.Buffer
	if err := Run(`print(algo.palindromeMinCuts("`+long[:5000]+`"), algo.palindromeValid("`+long+`"));`, &output); err != nil || output.String() != "0 true\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}

func TestFactorialsAreExact(t *testing.T) {
	want := big.NewInt(1)
	for n, term := range factorialTerms(maxFactorial) {
		if n > 0 {
			want.Mul(want, big.NewInt(int64(n)))
		}
		got, _ := new(big.Float).SetFloat64(float64(term.(object.Number))).Int(nil)
		if got.Cmp(want) != 0 {
			t.Fatalf("%d! = %s, want %s", n, got, want)
		}
	}
	want.Mul(want, big.NewInt(maxFactorial+1))
	if want.Cmp(big.NewInt(maxExactInteger)) <= 0 {
		t.Fatalf("%d! is a safe integer; raise maxFactorial", maxFactorial+1)
	}
}

func TestIsPrime64(t *testing.T) {
	for n := uint64(0); n < 20000; n++ {
		trial := n >= 2
		for divisor := uint64(2); divisor*divisor <= n && trial; divisor++ {
			trial = n%divisor != 0
		}
		if isPrime64(n) != trial {
			t.Fatalf("isPrime64(%d) = %v, want %v", n, !trial, trial)
		}
	}
	for n, want := range map[uint64]bool{
		561: false, 3215031751: false, 3825123056546413051: false, // strong pseudoprimes to several small bases
		4294967291: true, 4294967297: false, 999999999989: true,
		9007199254740881: true, 9007199254740991: false,
	} {
		if isPrime64(n) != want {
			t.Errorf("isPrime64(%d) = %v, want %v", n, !want, want)
		}
	}
}
