# The algo Module

`algo` is a built-in module of ready-made algorithms, available in every file without an import. Like [`math`](math.md), it is read-only: its functions can be called and stored like other values, but its members cannot be reassigned, and a local declaration named `algo` shadows it.

## Functions

| Function | Result |
| --- | --- |
| `algo.fibonacci(n)` | the Fibonacci number at index `n`, where index `0` is `0` and index `1` is `1` |
| `algo.fibonacciList(n)` | a new list of the Fibonacci numbers at indexes `0` through `n` (`n + 1` elements) |
| `algo.factorial(n)` | `n!`, the product `1 * 2 * ... * n`, where `0!` is `1` |
| `algo.factorialList(n)` | a new list of the factorials `0!` through `n!` (`n + 1` elements) |
| `algo.removeDuplicates(list)` | a new list with repeated values removed, keeping the first occurrence of each in order |
| `algo.findMatches(X, Y)` | a new list of the values in `X` that also appear in `Y`, without repeats, in `X`'s order |
| `algo.takeInventory(list)` | a new dictionary mapping each value to how many times it appears |
| `algo.findPlace(list, place)` | the value at `place` when distinct values are ranked largest first (`1` is the largest) |
| `algo.isPrime(x)` | `true` if the integer `x` is prime, otherwise `false` |
| `algo.rainwater(heights)` | the units of rain water trapped between the bars of an elevation map |
| `algo.medianSorted(nums1, nums2)` | the median of two sorted number lists combined, without merging them |
| `algo.mergeKSorted(lists)` | one sorted list merged from a list of sorted lists |
| `algo.editDistance(word1, word2)` | the fewest single-character inserts, deletes, or replacements that turn `word1` into `word2` |
| `algo.regexMatch(text, pattern)` | `true` if `pattern`, using `.` and `*`, matches all of `text` |
| `algo.slidingWindowMax(nums, k)` | the maximum of every window of `k` consecutive numbers, as a list |
| `algo.slidingWindowSumMax(nums, k)` | the largest sum of `k` consecutive numbers |
| `algo.slidingWindowMinLen(nums, target)` | the length of the shortest run of numbers whose sum is at least `target`, or `0` |
| `algo.slidingWindowLongestUnique(s)` | the length of the longest substring with no repeated character |
| `algo.slidingWindowMinSubstring(s, t)` | the shortest substring of `s` containing every character of `t`, or `""` |
| `algo.palindromeValid(s)` | `true` if `s` reads the same both ways, ignoring case and anything but letters and digits |
| `algo.palindromeCanBeValid(s)` | `true` if `s` is a palindrome after deleting at most one character |
| `algo.palindromeLongest(s)` | the longest palindromic substring of `s` |
| `algo.palindromeCount(s)` | how many substrings of `s` are palindromes |
| `algo.palindromeMinCuts(s)` | the fewest cuts that split `s` into palindromes |

Every function takes a fixed number of positional arguments and no keyword arguments, and none of them modifies its arguments.

## Fibonacci Numbers

```tg
var target = algo.fibonacci(6);
print(target);

// Get the full sequence up to index 6
var sequence = algo.fibonacciList(6);
print(sequence);
print(algo.fibonacci(0), algo.fibonacciList(0), algo.fibonacci(78));
```

```text
8
[0, 1, 1, 2, 3, 5, 8]
0 [0] 8944394323791464
```

`n` must be a non-negative whole number no greater than `78`, the largest index whose Fibonacci number Tiger's numbers represent exactly. Each `fibonacciList` call returns a fresh list that can be modified freely.

**Expected error:**

```tg
algo.fibonacci(79);
```

```error
algo.fibonacci: index 79 is too large; the maximum is 78
```

## Factorials

`algo.factorial(n)` and `algo.factorialList(n)` work like the Fibonacci pair: one returns `n!`, the other a list of `0!` through `n!`.

```tg
var target = algo.factorial(5);
print(target);

// Get every factorial from 0! up to 5!
var sequence = algo.factorialList(5);
print(sequence);
print(algo.factorial(0), algo.factorial(10), algo.factorial(18));
```

```text
120
[1, 1, 2, 6, 24, 120]
1 3628800 6402373705728000
```

`n` must be a non-negative whole number no greater than `18`. `18!` is the largest factorial below `9007199254740991`, the limit up to which Tiger's numbers hold and print every integer exactly. Each `factorialList` call returns a fresh list.

Factorials make counting problems short:

```tg
function choose(n, k) {
    return algo.factorial(n) / (algo.factorial(k) * algo.factorial(n - k));
}
print(choose(5, 2), choose(18, 9));
```

```text
10 48620
```

**Expected error:**

```tg
algo.factorial(19);
```

```error
algo.factorial: index 19 is too large; the maximum is 18
```

## Comparing Values

`removeDuplicates`, `findMatches`, `takeInventory`, and `findPlace` compare values with `==`:

- `1` and `1.0` are equal, but `1`, `"1"`, and `true` are distinct (unlike Python, where `1 == True`).
- Lists and dictionaries are compared by contents.
- Class instances are compared by identity, so two separate instances never match.

## Removing Duplicates

```tg
const visits = ["home", "about", "home", "blog", "about"];
print(algo.removeDuplicates(visits));
print(visits);
print(algo.removeDuplicates([3, 1, 3.0, "3", true, [1, 2], [1, 2]]));
```

```text
["home", "about", "blog"]
["home", "about", "home", "blog", "about"]
[3, 1, "3", true, [1, 2]]
```

## Finding Matches

`algo.findMatches(X, Y)` works like Python's `list(set(X) & set(Y))`, with two differences: the result keeps the order in which values first appear in `X`, and it also accepts lists and dictionaries as elements.

```tg
const mine = ["tea", "jam", "bread", "tea", "milk"];
const yours = ["milk", "eggs", "tea"];
print(algo.findMatches(mine, yours));
print(algo.findMatches(yours, mine));
print(algo.findMatches([1, 2, 3], [4, 5]));
print(algo.findMatches([[1, 2], {"a": 1}, 7], [7.0, {"a": 1}, [1, 2]]));
```

```text
["tea", "milk"]
["milk", "tea"]
[]
[[1, 2], {"a": 1}, 7]
```

Both arguments must be lists.

```tg
algo.findMatches([1, 2], "12");
```

```error
algo.findMatches expects a list, got string
```

## Taking Inventory

`algo.takeInventory(list)` counts how often each value occurs, like Python's `freq[item] = freq.get(item, 0) + 1` loop. Keys appear in the order each value is first seen, and values that are `==` share a count, so `1` and `1.0` are counted together.

```tg
const fruit = ["apple", "pear", "apple", "fig", "apple", "pear"];
const counts = algo.takeInventory(fruit);
print(counts);
print(counts["apple"], len(counts));
print(algo.takeInventory([1, 1.0, "1", true]), algo.takeInventory([]));
```

```text
{"apple": 3, "pear": 2, "fig": 1}
3 3
{1: 2, "1": 1, true: 1} {}
```

Elements must be usable as dictionary keys (numbers, strings, or booleans).

```tg
algo.takeInventory([[1], [1]]);
```

```error
algo.takeInventory: list cannot be a dictionary key
```

## Finding a Place

`algo.findPlace(list, place)` ranks the list's distinct values from largest to smallest and returns the one at `place`: `1` is first place (the largest), `2` is second place, and so on. Repeated values share one place, as with Python's `sorted(set(lst))`. Strings rank alphabetically, so the alphabetically last string is first place. The list itself is not changed.

```tg
const scores = [70, 95, 88, 95, 60, 88];
print(algo.findPlace(scores, 1), algo.findPlace(scores, 2), algo.findPlace(scores, 3));
print(algo.findPlace(["pear", "apple", "zebra"], 1));

try {
    algo.findPlace(scores, 5);
} catch (error) {
    print("no fifth place:", "out of range" in error);
}
```

```text
95 88 70
zebra
no fifth place: true
```

`place` must be a positive whole number no greater than the number of distinct values; anything else raises an error that `try`/`catch` can handle. The list must hold only numbers or only strings.

**Expected error:**

```tg
algo.findPlace([3, 1, 2], 4);
```

```error
algo.findPlace: place 4 is out of range; the list has 3 distinct values
```

## Prime Numbers

`algo.isPrime(x)` returns `true` when the integer `x` is prime and `false` otherwise. Numbers below `2`, including `0`, `1`, and negatives, are not prime. A whole-valued number such as `7.0` counts as an integer. The result is exact for every integer up to `9007199254740991`, the largest integer Tiger's numbers represent exactly.

```tg
var primes = [];
for n in range(30) {
    if algo.isPrime(n) {
        primes += [n];
    }
}
print(primes);
print(algo.isPrime(1), algo.isPrime(-7), algo.isPrime(7919), algo.isPrime(561));
```

```text
[2, 3, 5, 7, 11, 13, 17, 19, 23, 29]
false false true false
```

Fractions, strings, booleans, and integers beyond `9007199254740991` in either direction raise an error.

```tg
algo.isPrime(2.5);
```

```error
algo.isPrime expects an integer, got 2.5
```

## Trapping Rain Water

`algo.rainwater(heights)` solves the classic "trapping rain water" problem. Each number in the list is the height of a bar one unit wide, and the result is how much water collects between the bars after rain. It uses the two-pointer method, so it takes one pass over the list and no extra memory. The list itself is not changed.

```tg
var map = [0, 1, 0, 2, 1, 0, 1, 3, 2, 1, 2, 1];
var trapped = algo.rainwater(map);
print(trapped);
print(algo.rainwater([4, 2, 0, 3, 2, 5]), algo.rainwater([3, 0, 3]), algo.rainwater([1, 2, 3]), algo.rainwater([]));
```

```text
6
9 3 0 0
```

In the first map, water fills to the height of the lower of the tallest bars on each side (`~` is water):

```text
.......#....
...#~~~##~#.
.#~##~######
```

Heights must be non-negative numbers; fractions are allowed. An empty list or a list with fewer than three bars holds no water.

```tg
algo.rainwater([1, -2, 3]);
```

```error
algo.rainwater: heights cannot be negative, got -2 at index 1
```

## Median of Two Sorted Lists

`algo.medianSorted(nums1, nums2)` returns the median of all the numbers in two ascending lists, as in LeetCode #4. Instead of merging, it binary-searches where to split the shorter list, so the search takes $O(\log(\min(m, n)))$ steps. With an odd total count the median is the middle value; with an even count it is the average of the two middle values. Tiger has one number type, so a whole-number median prints without `.0`.

```tg
var medianValue = algo.medianSorted([1, 3], [2]);
var secondMedian = algo.medianSorted([1, 2], [3, 4]);
print(medianValue, secondMedian);
print(algo.medianSorted([], [5, 9]), algo.medianSorted([100], [1, 2, 3, 4, 5]));
```

```text
2 2.5
7 3.5
```

Both arguments must be lists of numbers in ascending order, and at least one must be non-empty. Each element is checked first, so an unsorted list raises an error instead of returning a wrong answer; that check reads every element once.

```tg
algo.medianSorted([3, 1], [2]);
```

```error
algo.medianSorted: first list is not sorted: 1 at index 1 comes after 3
```

## Merging k Sorted Lists

`algo.mergeKSorted(lists)` merges a list of ascending lists into one new ascending list, as in LeetCode #23. A min-heap holds the front element of each list and repeatedly takes the smallest, which takes $O(N \log k)$ time for $N$ total elements in $k$ lists. Equal values keep the order of the lists they came from, and the input lists are not changed.

```tg
var sortedLists = [
    [1, 4, 5],
    [1, 3, 4],
    [2, 6]
];
var mergedResult = algo.mergeKSorted(sortedLists);
print(mergedResult);
print(algo.mergeKSorted([["pear"], [], ["apple", "fig"]]), algo.mergeKSorted([]));
```

```text
[1, 1, 2, 3, 4, 4, 5, 6]
["apple", "fig", "pear"] []
```

Every inner list must be sorted ascending, and all elements must be numbers or all strings. Empty inner lists are allowed.

```tg
algo.mergeKSorted([[1, 5], [4, 2]]);
```

```error
algo.mergeKSorted: list 1 is not sorted: 2 at index 1 comes after 4
```

## Edit Distance

`algo.editDistance(word1, word2)` returns the Levenshtein distance, as in LeetCode #72: the minimum number of single-character insertions, deletions, and replacements that turn `word1` into `word2`. It fills the dynamic-programming table one row at a time, taking $O(m \cdot n)$ time and keeping only one row sized to the shorter word. Characters are Unicode code points, and the comparison is case-sensitive.

```tg
var totalOperations = algo.editDistance("horse", "ros");
print(totalOperations);
print(algo.editDistance("kitten", "sitting"), algo.editDistance("", "abc"), algo.editDistance("Tiger", "tiger"), algo.editDistance("café", "cafe"));
```

```text
3
3 3 1 1
```

It makes a handy spelling suggester:

```tg
const words = ["print", "range", "input", "return"];
function suggest(typo) {
    var best = words[0];
    for word in words {
        if algo.editDistance(typo, word) < algo.editDistance(typo, best) {
            best = word;
        }
    }
    return best;
}
print(suggest("pirnt"), suggest("imput"), suggest("retrun"));
```

```text
print input return
```

## Regular Expression Matching

`algo.regexMatch(text, pattern)` reports whether `pattern` matches the whole of `text`, as in LeetCode #10. The pattern supports two special characters:

- `.` matches any single character.
- `*` matches zero or more of the element just before it, so `a*` matches `""`, `"a"`, `"aa"`, and so on, and `.*` matches anything.

Every other character, including `+`, `?`, `(`, and `\`, matches only itself. The match must cover the entire text, as if the pattern were anchored with `^` and `$`. A dynamic-programming table over positions in the text and pattern handles the choices `*` introduces in $O(m \cdot n)$ time.

```tg
var matchesPattern = algo.regexMatch("aa", "a*");
print(matchesPattern);
print(algo.regexMatch("aa", "a"), algo.regexMatch("aab", "c*a*b"), algo.regexMatch("mississippi", "mis*is*p*."));
print(algo.regexMatch("main.tg", ".*.tg"), algo.regexMatch("", "x*"));
```

```text
true
false true false
true true
```

A `*` must follow a character or `.`; a pattern that starts with `*` or contains `**` raises an error.

```tg
algo.regexMatch("a", "*a");
```

```error
algo.regexMatch: * at index 0 must follow a character or .
```

Both functions take two strings. To bound the work, the text lengths plus one, multiplied together, may not exceed 25,000,000 (for example, two 5,000-character strings).

## Sliding Windows

A sliding window is a range of consecutive elements that moves across a list or string, so each step reuses the work of the last one instead of starting over. Each function solves a classic window problem in one pass:

| Function | Window | Problem | Time / extra space |
| --- | --- | --- | --- |
| `slidingWindowMax(nums, k)` | fixed size, monotonic queue | LeetCode #239 | $O(n)$ / $O(k)$ |
| `slidingWindowSumMax(nums, k)` | fixed size, running sum | LeetCode #643 | $O(n)$ / $O(1)$ |
| `slidingWindowMinLen(nums, target)` | grows, then shrinks while the sum reaches `target` | LeetCode #209 | $O(n)$ / $O(1)$ |
| `slidingWindowLongestUnique(s)` | grows, jumping past repeated characters | LeetCode #3 | $O(n)$ / $O(\min(m, n))$ |
| `slidingWindowMinSubstring(s, t)` | grows until it covers `t`, then shrinks | LeetCode #76 | $O(n + m)$ / $O(\vert\Sigma\vert)$ |

### Fixed-size windows

`slidingWindowMax(nums, k)` returns a list with the maximum of each window of `k` numbers, from left to right; the result has `len(nums) - k + 1` elements. `slidingWindowSumMax(nums, k)` returns the largest window sum; divide by `k` for the maximum average, as LeetCode #643 asks. `k` must be a whole number from `1` to `len(nums)`.

```tg
print(algo.slidingWindowMax([1, 3, -1, -3, 5, 3, 6, 7], 3));
print(algo.slidingWindowSumMax([1, 12, -5, -6, 50, 3], 4), algo.slidingWindowSumMax([1, 12, -5, -6, 50, 3], 4) / 4);
const temps = [18, 21, 19, 25, 24, 22, 27, 26];
print(algo.slidingWindowMax(temps, 3));
```

```text
[3, 3, 5, 5, 6, 7]
51 12.75
[21, 25, 25, 25, 27, 27]
```

### Shortest run reaching a target

`slidingWindowMinLen(nums, target)` returns the length of the shortest run of consecutive numbers whose sum is at least `target`, or `0` if no run is long enough. The numbers cannot be negative (the shrinking step relies on that), and `target` must be positive.

```tg
print(algo.slidingWindowMinLen([2, 3, 1, 2, 4, 3], 7), algo.slidingWindowMinLen([1, 4, 4], 4), algo.slidingWindowMinLen([1, 1, 1], 11));
```

```text
2 1 0
```

### Strings

`slidingWindowLongestUnique(s)` returns the length of the longest substring without a repeated character. `slidingWindowMinSubstring(s, t)` returns the shortest substring of `s` that contains every character of `t`, counting repeats, or `""` when there is none (or `t` is empty). When several windows tie, it returns the leftmost. Both compare Unicode characters case-sensitively.

```tg
print(algo.slidingWindowLongestUnique("abcabcbb"), algo.slidingWindowLongestUnique("bbbbb"), algo.slidingWindowLongestUnique("pwwkew"));
print(algo.slidingWindowMinSubstring("ADOBECODEBANC", "ABC"));
print(algo.slidingWindowMinSubstring("a", "aa") == "");
```

```text
3 1 3
BANC
true
```

**Expected error:**

```tg
algo.slidingWindowMax([1, 2], 3);
```

```error
algo.slidingWindowMax: k must be a whole number from 1 to 2 (the length of nums), got 3
```

## Palindromes

A palindrome reads the same forward and backward. These functions cover the classic palindrome problems:

| Function | Problem | Core mechanism | Time / extra space |
| --- | --- | --- | --- |
| `palindromeValid(s)` | LeetCode #125 | two pointers moving inward, skipping non-alphanumerics, ignoring case | $O(n)$ / $O(1)$ |
| `palindromeCanBeValid(s)` | LeetCode #680 | two pointers; at the first mismatch, try deleting either character | $O(n)$ / $O(1)$ |
| `palindromeLongest(s)` | LeetCode #5 | expand around each odd and even center | $O(n^2)$ / $O(1)$ |
| `palindromeCount(s)` | LeetCode #647 | expand around each center, counting every palindrome found | $O(n^2)$ / $O(1)$ |
| `palindromeMinCuts(s)` | LeetCode #132 | expand around centers while updating a minimum-cuts DP array | $O(n^2)$ / $O(n)$ |

`palindromeValid` is the only one that ignores case and punctuation; it treats Unicode letters and digits as alphanumeric. The other four compare characters exactly, so `"Aba"` is not a palindrome to them.

```tg
print(algo.palindromeValid("A man, a plan, a canal: Panama"), algo.palindromeValid("race a car"));
print(algo.palindromeCanBeValid("abca"), algo.palindromeCanBeValid("abc"));
print(algo.palindromeLongest("babad"), algo.palindromeLongest("cbbd"));
print(algo.palindromeCount("abc"), algo.palindromeCount("aaa"));
print(algo.palindromeMinCuts("aab"), algo.palindromeMinCuts("banana"));
```

```text
true false
true false
bab bb
3 6
1 1
```

`palindromeLongest` returns the leftmost palindrome when several share the longest length (`"babad"` gives `"bab"`, not `"aba"`) and `""` for an empty string. `palindromeCount` counts substrings by position, so the three `"a"`s in `"aaa"` count separately. `palindromeMinCuts` returns `0` when `s` is already a palindrome or empty; `"aab"` needs one cut, `"aa" | "b"`.

The three $O(n^2)$ functions accept strings up to 5,000 characters, which keeps a single call to about 25 million steps.

```tg
algo.palindromeCount(121);
```

```error
algo.palindromeCount expects a string, got number
```

See also the [math module](math.md), the other [built-ins](builtins.md), and the algo benchmarks: [24_algo.tg](../benchmarks/24_algo.tg), [inventory and places](../benchmarks/25_algo_inventory.tg), [rainwater](../benchmarks/26_algo_rainwater.tg), [sorted lists](../benchmarks/27_algo_sorted.tg), [string algorithms](../benchmarks/28_algo_strings.tg), [sliding windows](../benchmarks/29_algo_windows.tg), [palindromes](../benchmarks/30_algo_palindromes.tg), and [factorials](../benchmarks/31_algo_factorials.tg).
