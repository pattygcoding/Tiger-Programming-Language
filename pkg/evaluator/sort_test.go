package evaluator

import (
	"sort"
	"testing"
)

func TestStableSortMatchesStandardLibrary(t *testing.T) {
	values := []int{5, 3, 8, 1, 9, 2, 7, 4, 6, 0, 15, 13, 18, 11, 19, 12, 17, 14, 16, 10}
	want := append([]int(nil), values...)
	sort.Ints(want)
	stableSort(values, func(left, right int) bool { return left < right })
	for index := range values {
		if values[index] != want[index] {
			t.Fatalf("index %d = %d; want %d (%v)", index, values[index], want[index], values)
		}
	}
}

func TestStableSortPreservesEqualOrder(t *testing.T) {
	type pair struct{ key, tag int }
	items := []pair{{2, 0}, {1, 1}, {2, 2}, {1, 3}, {2, 4}, {1, 5}}
	stableSort(items, func(left, right pair) bool { return left.key < right.key })
	want := []int{1, 3, 5, 0, 2, 4}
	for index, item := range items {
		if item.tag != want[index] {
			t.Fatalf("position %d tag = %d; want %d", index, item.tag, want[index])
		}
	}
}

func TestStableSortEdgeCases(t *testing.T) {
	for name, values := range map[string][]int{
		"empty":    {},
		"single":   {7},
		"equal":    {4, 4, 4, 4},
		"two":      {2, 1},
		"multiple": {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14},
	} {
		got := append([]int(nil), values...)
		want := append([]int(nil), values...)
		sort.Ints(want)
		stableSort(got, func(left, right int) bool { return left < right })
		for index := range want {
			if got[index] != want[index] {
				t.Fatalf("%s: %v sorted to %v; want %v", name, values, got, want)
			}
		}
	}
}

func TestStableSortDescending(t *testing.T) {
	values := []int{3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5}
	stableSort(values, func(left, right int) bool { return left > right })
	for index := 1; index < len(values); index++ {
		if values[index-1] < values[index] {
			t.Fatalf("not descending at %d: %v", index, values)
		}
	}
}
