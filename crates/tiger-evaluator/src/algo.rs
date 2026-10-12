//! The `algo` module: a suite of classic algorithms.
//!
//! A direct port of `pkg/evaluator/algomodule.go`. Every function keeps the Go
//! implementation's argument validation, limits, and error messages.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use tiger_object as object;
use tiger_object::{Module, Value};

/// The largest index whose Fibonacci number is exact in a float64.
const MAX_FIBONACCI: usize = 78;
/// The largest `n` with `n!` <= 2^53 - 1, so every result is exact.
const MAX_FACTORIAL: usize = 18;
/// Bounds the O(n^2) palindrome scans.
const MAX_QUADRATIC_LENGTH: usize = 5000;
/// Bounds the `(m+1)*(n+1)` work of the string algorithms.
const MAX_DP_CELLS: usize = 25_000_000;
/// 2^53 - 1, the largest integer every value below which is exact in a float64.
const MAX_EXACT_INTEGER: f64 = 9007199254740991.0;

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

/// Wraps a function that receives the leading list arguments plus the rest.
fn list_builtin<F>(name: &str, arity: usize, list_count: usize, function: F) -> Rc<object::Builtin>
where
    F: Fn(&[Rc<RefCell<Vec<Value>>>], &[Value]) -> Result<Value, String> + 'static,
{
    let qualified = format!("algo.{}", name);
    let label = qualified.clone();
    object::Builtin::new(&label, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", qualified));
        }
        if arguments.len() != arity {
            return Err(format!("{} expects {} argument{}, got {}", qualified, arity, plural(arity), arguments.len()));
        }
        let mut lists = Vec::with_capacity(list_count);
        for argument in &arguments[..list_count] {
            match argument {
                Value::List(list) => lists.push(list.clone()),
                other => return Err(format!("{} expects a list, got {}", qualified, other.type_name())),
            }
        }
        function(&lists, &arguments[list_count..]).map_err(|message| format!("{}: {}", qualified, message))
    })
}

/// Wraps a function that receives `count` string arguments as rune vectors.
fn strings_builtin<F>(name: &str, count: usize, limit_cells: bool, function: F) -> Rc<object::Builtin>
where
    F: Fn(&[Vec<char>]) -> Result<Value, String> + 'static,
{
    let qualified = format!("algo.{}", name);
    let label = qualified.clone();
    object::Builtin::new(&label, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", qualified));
        }
        if arguments.len() != count {
            return Err(format!("{} expects {} argument{}, got {}", qualified, count, plural(count), arguments.len()));
        }
        let mut words = Vec::with_capacity(count);
        let mut cells = 1usize;
        for argument in arguments {
            match argument {
                Value::String(text) => {
                    let runes: Vec<char> = text.chars().collect();
                    cells = cells.saturating_mul(runes.len() + 1);
                    words.push(runes);
                }
                other if count == 1 => return Err(format!("{} expects a string, got {}", qualified, other.type_name())),
                _ => {
                    return Err(format!(
                        "{} expects two strings, got {} and {}",
                        qualified,
                        arguments[0].type_name(),
                        arguments[1].type_name()
                    ))
                }
            }
        }
        if limit_cells && cells > MAX_DP_CELLS {
            return Err(format!("{}: inputs are too long ({} table cells; the limit is {})", qualified, cells, MAX_DP_CELLS));
        }
        function(&words).map_err(|message| format!("{}: {}", qualified, message))
    })
}

/// A set of values compared with `==`: scalars hash, other values are scanned.
#[derive(Default)]
struct ValueSet {
    hashed: HashSet<object::Key>,
    unhashable: Vec<Value>,
}

impl ValueSet {
    fn contains(&self, value: &Value) -> bool {
        match object::hash(value) {
            Ok(key) => self.hashed.contains(&key),
            Err(_) => self.unhashable.iter().any(|kept| object::equal(kept, value)),
        }
    }

    fn add(&mut self, value: &Value) {
        if self.contains(value) {
            return;
        }
        match object::hash(value) {
            Ok(key) => {
                self.hashed.insert(key);
            }
            Err(_) => self.unhashable.push(value.clone()),
        }
    }
}

/// Returns the first occurrence of each element that `keep` accepts, in order.
fn unique_where<F>(list: &Rc<RefCell<Vec<Value>>>, keep: F) -> Vec<Value>
where
    F: Fn(&Value) -> bool,
{
    let mut seen = ValueSet::default();
    let mut result = Vec::new();
    for element in list.borrow().iter() {
        if seen.contains(element) || !keep(element) {
            continue;
        }
        seen.add(element);
        result.push(element.clone());
    }
    result
}

/// Converts a list to numbers, naming the first non-number.
fn numbers_of(list: &Rc<RefCell<Vec<Value>>>) -> Result<Vec<f64>, String> {
    let mut numbers = Vec::new();
    for (index, element) in list.borrow().iter().enumerate() {
        match element {
            Value::Number(number) => numbers.push(*number),
            other => return Err(format!("nums must contain numbers, got {} at index {}", other.type_name(), index)),
        }
    }
    Ok(numbers)
}

/// Compares two numbers or two strings.
fn less_value(left: &Value, right: &Value) -> bool {
    match left {
        Value::Number(number) => match right {
            Value::Number(other) => number < other,
            _ => false,
        },
        _ => match (left, right) {
            (Value::String(first), Value::String(second)) => first < second,
            _ => false,
        },
    }
}

/// Validates a single index argument and returns either its term or the list of
/// terms `0..n`.
fn sequence_builtin(name: &str, limit: usize, list: bool, terms: fn(usize) -> Vec<Value>) -> Rc<object::Builtin> {
    let qualified = format!("algo.{}", name);
    let label = qualified.clone();
    object::Builtin::new(&label, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", qualified));
        }
        if arguments.len() != 1 {
            return Err(format!("{} expects 1 argument, got {}", qualified, arguments.len()));
        }
        let Value::Number(index) = &arguments[0] else {
            return Err(format!("{} expects a number, got {}", qualified, arguments[0].type_name()));
        };
        if *index < 0.0 || index.trunc() != *index {
            return Err(format!("{} expects a non-negative integer, got {}", qualified, object::format(&arguments[0])));
        }
        if *index > limit as f64 {
            return Err(format!("{}: index {} is too large; the maximum is {}", qualified, object::format(&arguments[0]), limit));
        }
        let sequence = terms(*index as usize);
        if list {
            return Ok(Value::list(sequence));
        }
        Ok(sequence[sequence.len() - 1].clone())
    })
}

/// Validates `(nums, k)` with `1 <= k <= len(nums)` for fixed-size windows.
fn window_builtin(name: &str, function: fn(&[f64], usize) -> Value) -> Rc<object::Builtin> {
    list_builtin(name, 2, 1, move |lists, rest| {
        let numbers = numbers_of(&lists[0])?;
        let Value::Number(size) = &rest[0] else {
            return Err(format!("k must be a number, got {}", rest[0].type_name()));
        };
        if size.trunc() != *size || *size < 1.0 || *size > numbers.len() as f64 {
            return Err(format!(
                "k must be a whole number from 1 to {} (the length of nums), got {}",
                numbers.len(),
                object::format(&rest[0])
            ));
        }
        Ok(function(&numbers, *size as usize))
    })
}

/// A two-string algorithm with the `(m+1)*(n+1)` cell limit applied.
fn string_pair_builtin(name: &str, function: fn(&[Vec<char>]) -> Result<Value, String>) -> Rc<object::Builtin> {
    strings_builtin(name, 2, true, function)
}

/// A one-string algorithm bounded by [`MAX_QUADRATIC_LENGTH`].
fn quadratic_string_builtin(name: &str, function: fn(&[char]) -> Value) -> Rc<object::Builtin> {
    strings_builtin(name, 1, false, move |words| {
        if words[0].len() > MAX_QUADRATIC_LENGTH {
            return Err(format!("input is too long ({} characters; the limit is {})", words[0].len(), MAX_QUADRATIC_LENGTH));
        }
        Ok(function(&words[0]))
    })
}

/// The `algo` module.
fn fibonacci_terms(n: usize) -> Vec<Value> {
    let mut terms = Vec::with_capacity(n + 1);
    let (mut previous, mut current) = (0.0, 1.0);
    for _ in 0..=n {
        terms.push(Value::Number(previous));
        let next = previous + current;
        previous = current;
        current = next;
    }
    terms
}

fn factorial_terms(n: usize) -> Vec<Value> {
    let mut terms = Vec::with_capacity(n + 1);
    let mut product = 1.0;
    for index in 0..=n {
        if index > 0 {
            product *= index as f64;
        }
        terms.push(Value::Number(product));
    }
    terms
}

/// The simple case mapping Go's `unicode.ToLower` uses.
fn lower(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

/// Compares letters and digits from both ends, ignoring case and everything else.
fn palindrome_valid(text: &[char]) -> bool {
    let (mut left, mut right) = (0i64, text.len() as i64 - 1);
    while left < right {
        while left < right && !text[left as usize].is_alphanumeric() {
            left += 1;
        }
        while left < right && !text[right as usize].is_alphanumeric() {
            right -= 1;
        }
        if lower(text[left as usize]) != lower(text[right as usize]) {
            return false;
        }
        left += 1;
        right -= 1;
    }
    true
}

fn is_palindrome(text: &[char], mut left: i64, mut right: i64) -> bool {
    while left < right {
        if text[left as usize] != text[right as usize] {
            return false;
        }
        left += 1;
        right -= 1;
    }
    true
}

/// Allows deleting one character at the first mismatch, trying either side.
fn palindrome_can_be_valid(text: &[char]) -> bool {
    let (mut left, mut right) = (0i64, text.len() as i64 - 1);
    while left < right {
        if text[left as usize] != text[right as usize] {
            return is_palindrome(text, left + 1, right) || is_palindrome(text, left, right - 1);
        }
        left += 1;
        right -= 1;
    }
    true
}

/// Grows a palindrome outward from `text[left..right]` and returns its bounds.
fn expand(text: &[char], mut left: i64, mut right: i64) -> (usize, usize) {
    while left >= 0 && right < text.len() as i64 && text[left as usize] == text[right as usize] {
        left -= 1;
        right += 1;
    }
    ((left + 1) as usize, (right - left - 1) as usize)
}

/// Expands around all 2n-1 centers and keeps the first longest palindrome.
fn palindrome_longest(text: &[char]) -> Value {
    let (mut best_start, mut best_length) = (0usize, 0usize);
    for center in 0..text.len() as i64 {
        for even in 0..2 {
            let (start, length) = expand(text, center, center + even);
            if length > best_length {
                best_start = start;
                best_length = length;
            }
        }
    }
    Value::string(&text[best_start..best_start + best_length].iter().collect::<String>())
}

fn palindrome_count(text: &[char]) -> Value {
    let mut count = 0.0;
    for center in 0..text.len() as i64 {
        for even in 0..2 {
            let (mut left, mut right) = (center, center + even);
            while left >= 0 && right < text.len() as i64 && text[left as usize] == text[right as usize] {
                count += 1.0;
                left -= 1;
                right += 1;
            }
        }
    }
    Value::Number(count)
}

/// `cuts[i]` is the fewest cuts for `text[:i]`, with `cuts[0] = -1`.
fn palindrome_min_cuts(text: &[char]) -> Value {
    let mut cuts: Vec<i64> = (0..=text.len() as i64).map(|index| index - 1).collect();
    for center in 0..text.len() as i64 {
        for even in 0..2 {
            let (mut left, mut right) = (center, center + even);
            while left >= 0 && right < text.len() as i64 && text[left as usize] == text[right as usize] {
                cuts[right as usize + 1] = cuts[right as usize + 1].min(cuts[left as usize] + 1);
                left -= 1;
                right += 1;
            }
        }
    }
    Value::Number(cuts[text.len()].max(0) as f64)
}

pub fn algo_module() -> Rc<Module> {
    let definitions: Vec<(&str, Rc<object::Builtin>)> = vec![
        ("fibonacci", sequence_builtin("fibonacci", MAX_FIBONACCI, false, fibonacci_terms)),
        ("fibonacciList", sequence_builtin("fibonacciList", MAX_FIBONACCI, true, fibonacci_terms)),
        ("factorial", sequence_builtin("factorial", MAX_FACTORIAL, false, factorial_terms)),
        ("factorialList", sequence_builtin("factorialList", MAX_FACTORIAL, true, factorial_terms)),
        ("removeDuplicates", list_builtin("removeDuplicates", 1, 1, |lists, _| Ok(Value::list(unique_where(&lists[0], |_| true))))),
        (
            "findMatches",
            list_builtin("findMatches", 2, 2, |lists, _| {
                let mut other = ValueSet::default();
                for element in lists[1].borrow().iter() {
                    other.add(element);
                }
                Ok(Value::list(unique_where(&lists[0], |value| other.contains(value))))
            }),
        ),
        ("takeInventory", list_builtin("takeInventory", 1, 1, take_inventory)),
        ("findPlace", list_builtin("findPlace", 2, 1, find_place)),
        ("isPrime", object::Builtin::new("algo.isPrime", is_prime)),
        ("rainwater", list_builtin("rainwater", 1, 1, rainwater)),
        ("medianSorted", list_builtin("medianSorted", 2, 2, median_sorted)),
        ("mergeKSorted", list_builtin("mergeKSorted", 1, 1, merge_k_sorted)),
        ("editDistance", string_pair_builtin("editDistance", edit_distance)),
        ("regexMatch", string_pair_builtin("regexMatch", regex_match)),
        ("slidingWindowMax", window_builtin("slidingWindowMax", sliding_window_max)),
        ("slidingWindowSumMax", window_builtin("slidingWindowSumMax", sliding_window_sum_max)),
        ("slidingWindowMinLen", list_builtin("slidingWindowMinLen", 2, 1, sliding_window_min_len)),
        (
            "slidingWindowLongestUnique",
            strings_builtin("slidingWindowLongestUnique", 1, false, |words| Ok(sliding_window_longest_unique(&words[0]))),
        ),
        (
            "slidingWindowMinSubstring",
            strings_builtin("slidingWindowMinSubstring", 2, false, |words| Ok(sliding_window_min_substring(&words[0], &words[1]))),
        ),
        ("palindromeValid", strings_builtin("palindromeValid", 1, false, |words| Ok(Value::Bool(palindrome_valid(&words[0]))))),
        (
            "palindromeCanBeValid",
            strings_builtin("palindromeCanBeValid", 1, false, |words| Ok(Value::Bool(palindrome_can_be_valid(&words[0])))),
        ),
        ("palindromeLongest", quadratic_string_builtin("palindromeLongest", palindrome_longest)),
        ("palindromeCount", quadratic_string_builtin("palindromeCount", palindrome_count)),
        ("palindromeMinCuts", quadratic_string_builtin("palindromeMinCuts", palindrome_min_cuts)),
    ];
    let env = object::Environment::new(None);
    for (name, builtin) in definitions {
        let _ = object::define(&env, name, Value::Builtin(builtin), true);
    }
    Rc::new(Module { name: "algo".to_string(), env })
}

/// Keeps a deque of indexes whose values decrease, so each window's maximum is at the front.
fn sliding_window_max(numbers: &[f64], k: usize) -> Value {
    let mut result = Vec::with_capacity(numbers.len() - k + 1);
    let mut deque: Vec<usize> = Vec::with_capacity(k);
    for (index, number) in numbers.iter().enumerate() {
        if !deque.is_empty() && deque[0] + k <= index {
            deque.remove(0);
        }
        while let Some(&last) = deque.last() {
            if numbers[last] <= *number {
                deque.pop();
            } else {
                break;
            }
        }
        deque.push(index);
        if index >= k - 1 {
            result.push(Value::Number(numbers[deque[0]]));
        }
    }
    Value::list(result)
}

fn sliding_window_sum_max(numbers: &[f64], k: usize) -> Value {
    let mut sum: f64 = numbers[..k].iter().sum();
    let mut best = sum;
    for index in k..numbers.len() {
        sum += numbers[index] - numbers[index - k];
        best = best.max(sum);
    }
    Value::Number(best)
}

/// Grows the window to reach target, then shrinks it from the left while it still does.
fn sliding_window_min_len(lists: &[Rc<RefCell<Vec<Value>>>], rest: &[Value]) -> Result<Value, String> {
    let numbers = numbers_of(&lists[0])?;
    for (index, number) in numbers.iter().enumerate() {
        if *number < 0.0 {
            return Err(format!(
                "nums cannot contain negative numbers, got {} at index {}",
                object::format(&Value::Number(*number)),
                index
            ));
        }
    }
    let Value::Number(target) = &rest[0] else {
        return Err(format!("target must be a number, got {}", rest[0].type_name()));
    };
    if *target <= 0.0 {
        return Err(format!("target must be positive, got {}", object::format(&rest[0])));
    }
    let (mut best, mut left, mut sum) = (0usize, 0usize, 0.0);
    for (right, number) in numbers.iter().enumerate() {
        sum += number;
        while sum >= *target {
            let length = right - left + 1;
            if best == 0 || length < best {
                best = length;
            }
            sum -= numbers[left];
            left += 1;
        }
    }
    Ok(Value::Number(best as f64))
}

/// Jumps the left edge past the previous occurrence of a repeated character.
fn sliding_window_longest_unique(text: &[char]) -> Value {
    let mut last: HashMap<char, usize> = HashMap::new();
    let (mut best, mut left) = (0usize, 0usize);
    for (right, character) in text.iter().enumerate() {
        if let Some(previous) = last.get(character) {
            if *previous >= left {
                left = *previous + 1;
            }
        }
        last.insert(*character, right);
        best = best.max(right - left + 1);
    }
    Value::Number(best as f64)
}

/// Returns the shortest, then leftmost, window of text holding every character of target.
fn sliding_window_min_substring(text: &[char], target: &[char]) -> Value {
    if target.is_empty() {
        return Value::string("");
    }
    let mut need: HashMap<char, i64> = HashMap::new();
    for character in target {
        *need.entry(*character).or_insert(0) += 1;
    }
    let mut missing = target.len() as i64;
    let (mut best_start, mut best_length, mut left) = (0usize, 0usize, 0usize);
    for (right, character) in text.iter().enumerate() {
        if need.get(character).copied().unwrap_or(0) > 0 {
            missing -= 1;
        }
        *need.entry(*character).or_insert(0) -= 1;
        while missing == 0 {
            let length = right - left + 1;
            if best_length == 0 || length < best_length {
                best_start = left;
                best_length = length;
            }
            *need.entry(text[left]).or_insert(0) += 1;
            if need[&text[left]] > 0 {
                missing += 1;
            }
            left += 1;
        }
    }
    Value::string(&text[best_start..best_start + best_length].iter().collect::<String>())
}
/// Levenshtein distance with one DP row sized to the shorter word.
fn edit_distance(words: &[Vec<char>]) -> Result<Value, String> {
    let (first, second) = (&words[0], &words[1]);
    let (long, short) = if first.len() < second.len() { (second, first) } else { (first, second) };
    let mut row: Vec<usize> = (0..=short.len()).collect();
    for i in 1..=long.len() {
        let mut diagonal = row[0];
        row[0] = i;
        for j in 1..=short.len() {
            let above = row[j];
            row[j] = if long[i - 1] == short[j - 1] { diagonal } else { 1 + diagonal.min(above).min(row[j - 1]) };
            diagonal = above;
        }
    }
    Ok(Value::Number(row[short.len()] as f64))
}

/// Reports whether pattern (with `.` and `*`) matches all of text, by a DP table over suffixes.
fn regex_match(words: &[Vec<char>]) -> Result<Value, String> {
    let (text, pattern) = (&words[0], &words[1]);
    for (index, character) in pattern.iter().enumerate() {
        if *character == '*' && (index == 0 || pattern[index - 1] == '*') {
            return Err(format!("* at index {} must follow a character or .", index));
        }
    }
    // matches[i][j] reports whether text[i:] matches pattern[j:].
    let mut matches = vec![vec![false; pattern.len() + 1]; text.len() + 1];
    matches[text.len()][pattern.len()] = true;
    for i in (0..=text.len()).rev() {
        for j in (0..pattern.len()).rev() {
            let first = i < text.len() && (pattern[j] == '.' || pattern[j] == text[i]);
            if j + 1 < pattern.len() && pattern[j + 1] == '*' {
                matches[i][j] = matches[i][j + 2] || (first && matches[i + 1][j]);
            } else {
                matches[i][j] = first && matches[i + 1][j + 1];
            }
        }
    }
    Ok(Value::Bool(matches[0][0]))
}

/// Checks that a list holds numbers in ascending order.
fn sorted_numbers(label: &str, list: &Rc<RefCell<Vec<Value>>>) -> Result<Vec<f64>, String> {
    let mut numbers: Vec<f64> = Vec::new();
    for (index, element) in list.borrow().iter().enumerate() {
        let Value::Number(number) = element else {
            return Err(format!("{} must contain numbers, got {} at index {}", label, element.type_name(), index));
        };
        if index > 0 && *number < numbers[index - 1] {
            return Err(format!(
                "{} is not sorted: {} at index {} comes after {}",
                label,
                object::format(element),
                index,
                object::format(&Value::Number(numbers[index - 1]))
            ));
        }
        numbers.push(*number);
    }
    Ok(numbers)
}

/// Binary-searches the partition of the shorter list, taking O(log(min(m, n))) steps.
fn median_sorted(lists: &[Rc<RefCell<Vec<Value>>>], _rest: &[Value]) -> Result<Value, String> {
    let first = sorted_numbers("first list", &lists[0])?;
    let second = sorted_numbers("second list", &lists[1])?;
    let (first, second) = if first.len() > second.len() { (second, first) } else { (first, second) };
    let (m, n) = (first.len() as i64, second.len() as i64);
    if m + n == 0 {
        return Err("both lists are empty".to_string());
    }
    let half = (m + n + 1) / 2;
    let (mut low, mut high) = (0i64, m);
    while low <= high {
        let cut = (low + high) / 2;
        let other = half - cut;
        let left_a = if cut > 0 { first[(cut - 1) as usize] } else { f64::NEG_INFINITY };
        let right_a = if cut < m { first[cut as usize] } else { f64::INFINITY };
        let left_b = if other > 0 { second[(other - 1) as usize] } else { f64::NEG_INFINITY };
        let right_b = if other < n { second[other as usize] } else { f64::INFINITY };
        if left_a > right_b {
            high = cut - 1;
        } else if left_b > right_a {
            low = cut + 1;
        } else {
            let left = left_a.max(left_b);
            if (m + n) % 2 == 1 {
                return Ok(Value::Number(left));
            }
            return Ok(Value::Number((left + right_a.min(right_b)) / 2.0));
        }
    }
    Err("lists must be sorted".to_string())
}

/// Merges sorted lists by the `(value, list)` key the Go min-heap orders on.
fn merge_k_sorted(lists: &[Rc<RefCell<Vec<Value>>>], _rest: &[Value]) -> Result<Value, String> {
    let mut items: Vec<(Value, usize)> = Vec::new();
    let mut kind = String::new();
    for (position, element) in lists[0].borrow().iter().enumerate() {
        let Value::List(inner) = element else {
            return Err(format!("expects a list of lists, got {} at index {}", element.type_name(), position));
        };
        let inner = inner.borrow();
        for (index, value) in inner.iter().enumerate() {
            match value {
                Value::Number(_) | Value::String(_) => {}
                other => {
                    return Err(format!("list {} must contain numbers or strings, got {} at index {}", position, other.type_name(), index))
                }
            }
            if kind.is_empty() {
                kind = value.type_name().to_string();
            } else if value.type_name() != kind {
                return Err(format!("cannot compare {} and {} (list {}, index {})", kind, value.type_name(), position, index));
            }
            if index > 0 && less_value(value, &inner[index - 1]) {
                return Err(format!(
                    "list {} is not sorted: {} at index {} comes after {}",
                    position,
                    object::format(value),
                    index,
                    object::format(&inner[index - 1])
                ));
            }
        }
        for value in inner.iter() {
            items.push((value.clone(), position));
        }
    }
    items.sort_by(|left, right| {
        if less_value(&left.0, &right.0) {
            return std::cmp::Ordering::Less;
        }
        if less_value(&right.0, &left.0) {
            return std::cmp::Ordering::Greater;
        }
        left.1.cmp(&right.1)
    });
    Ok(Value::list(items.into_iter().map(|(value, _)| value).collect()))
}

/// Solves "trapping rain water" with two pointers in O(n) time and O(1) space.
fn rainwater(lists: &[Rc<RefCell<Vec<Value>>>], _rest: &[Value]) -> Result<Value, String> {
    let mut heights: Vec<f64> = Vec::new();
    for (index, element) in lists[0].borrow().iter().enumerate() {
        let Value::Number(number) = element else {
            return Err(format!("heights must be numbers, got {} at index {}", element.type_name(), index));
        };
        if *number < 0.0 {
            return Err(format!("heights cannot be negative, got {} at index {}", object::format(element), index));
        }
        heights.push(*number);
    }
    if heights.is_empty() {
        return Ok(Value::Number(0.0));
    }
    let (mut left, mut right) = (0usize, heights.len() - 1);
    let (mut left_max, mut right_max) = (heights[left], heights[right]);
    let mut total = 0.0;
    while left < right {
        if left_max < right_max {
            left += 1;
            left_max = left_max.max(heights[left]);
            total += left_max - heights[left];
        } else {
            right -= 1;
            right_max = right_max.max(heights[right]);
            total += right_max - heights[right];
        }
    }
    Ok(Value::Number(total))
}

/// The first twelve prime bases; Miller-Rabin with them is exact below 3.3e24.
const PRIME_BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];

fn mul_mod(a: u64, b: u64, modulus: u64) -> u64 {
    ((a as u128 * b as u128) % modulus as u128) as u64
}

fn pow_mod(base: u64, mut exponent: u64, modulus: u64) -> u64 {
    let mut result = 1u64;
    let mut base = base % modulus;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = mul_mod(result, base, modulus);
        }
        base = mul_mod(base, base, modulus);
        exponent >>= 1;
    }
    result
}

fn is_prime64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for base in PRIME_BASES {
        if n % base == 0 {
            return n == base;
        }
    }
    let mut d = n - 1;
    let mut rounds = 0;
    while d % 2 == 0 {
        d /= 2;
        rounds += 1;
    }
    for base in PRIME_BASES {
        let mut x = pow_mod(base, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        let mut witness = true;
        let mut round = 1;
        while round < rounds && witness {
            x = mul_mod(x, x, n);
            witness = x != n - 1;
            round += 1;
        }
        if witness {
            return false;
        }
    }
    true
}

fn is_prime(_host: &mut dyn object::Host, arguments: &[Value], keywords: &[object::Keyword]) -> Result<Value, String> {
    const NAME: &str = "algo.isPrime";
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", NAME));
    }
    if arguments.len() != 1 {
        return Err(format!("{} expects 1 argument, got {}", NAME, arguments.len()));
    }
    let Value::Number(number) = &arguments[0] else {
        return Err(format!("{} expects an integer, got {}", NAME, arguments[0].type_name()));
    };
    if number.trunc() != *number {
        return Err(format!("{} expects an integer, got {}", NAME, object::format(&arguments[0])));
    }
    if number.abs() > MAX_EXACT_INTEGER {
        return Err(format!("{}: {} is too large; the maximum is {}", NAME, object::format(&arguments[0]), MAX_EXACT_INTEGER as u64));
    }
    Ok(Value::Bool(*number > 1.0 && is_prime64(*number as u64)))
}

/// Counts each value, keyed in first-occurrence order.
fn take_inventory(lists: &[Rc<RefCell<Vec<Value>>>], _rest: &[Value]) -> Result<Value, String> {
    let mut counts = object::Dict::default();
    for element in lists[0].borrow().iter() {
        let element = match element {
            Value::Number(number) => Value::Number(number + 0.0),
            other => other.clone(),
        };
        let count = match counts.get(&element)? {
            Some(Value::Number(number)) => number,
            _ => 0.0,
        };
        counts.set(element, Value::Number(count + 1.0))?;
    }
    Ok(Value::Dict(Rc::new(RefCell::new(counts))))
}

/// Returns the value ranked at place among distinct values, largest first.
fn find_place(lists: &[Rc<RefCell<Vec<Value>>>], rest: &[Value]) -> Result<Value, String> {
    let Value::Number(place) = &rest[0] else {
        return Err(format!("place must be a number, got {}", rest[0].type_name()));
    };
    if *place < 1.0 || place.trunc() != *place {
        return Err(format!("place must be a positive whole number, got {}", object::format(&rest[0])));
    }
    let mut values = unique_where(&lists[0], |_| true);
    for value in &values {
        match value {
            Value::Number(_) | Value::String(_) => {}
            other => return Err(format!("cannot rank {} values; use numbers or strings", other.type_name())),
        }
        if value.type_name() != values[0].type_name() {
            return Err(format!("cannot compare {} and {}", values[0].type_name(), value.type_name()));
        }
    }
    if *place > values.len() as f64 {
        return Err(format!(
            "place {} is out of range; the list has {} distinct value{}",
            object::format(&rest[0]),
            values.len(),
            plural(values.len())
        ));
    }
    values.sort_by(|left, right| match (left, right) {
        (Value::Number(first), Value::Number(second)) => second.partial_cmp(first).unwrap_or(std::cmp::Ordering::Equal),
        (Value::String(first), Value::String(second)) => second.cmp(first),
        _ => std::cmp::Ordering::Equal,
    });
    Ok(values[*place as usize - 1].clone())
}
