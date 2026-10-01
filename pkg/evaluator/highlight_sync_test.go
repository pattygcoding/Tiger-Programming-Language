package evaluator

import (
	"encoding/json"
	"os"
	"regexp"
	"sort"
	"strings"
	"testing"
)

func sortedNames(names map[string]bool) []string {
	list := make([]string, 0, len(names))
	for name := range names {
		list = append(list, name)
	}
	sort.Strings(list)
	return list
}

func readTestFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

// The playground highlighter and VS Code grammar must list exactly the math and algo members.
func TestHighlightersListModuleMembers(t *testing.T) {
	want := map[string]map[string]bool{"math": {}, "algo": {}}
	for name := range unaryMath {
		want["math"][name] = true
	}
	for name := range binaryMath {
		want["math"][name] = true
	}
	algo := algoModule()
	for _, match := range regexp.MustCompile(`env\.Define\("(\w+)"`).FindAllStringSubmatch(readTestFile(t, "algomodule.go"), -1) {
		if _, ok := algo.Env.GetOwn(match[1]); !ok {
			t.Fatalf("algo.%s is not defined", match[1])
		}
		want["algo"][match[1]] = true
	}

	playground := readTestFile(t, "../../web/highlight.mjs")
	var grammar struct {
		Repository map[string]struct {
			Patterns []struct {
				Match string `json:"match"`
			} `json:"patterns"`
		} `json:"repository"`
	}
	if err := json.Unmarshal([]byte(readTestFile(t, "../../editors/vscode/syntaxes/tiger.tmLanguage.json")), &grammar); err != nil {
		t.Fatal(err)
	}
	grammarMembers := map[string]string{}
	for _, pattern := range grammar.Repository["module-members"].Patterns {
		parts := regexp.MustCompile(`\((math|algo)\).*\(([\w|]+)\)\(\?!`).FindStringSubmatch(pattern.Match)
		if parts == nil {
			t.Fatalf("unrecognized module pattern %q", pattern.Match)
		}
		grammarMembers[parts[1]] = parts[2]
	}

	for module, members := range want {
		block := regexp.MustCompile(`(?s)` + module + `: new Set\(\[(.*?)\]\)`).FindStringSubmatch(playground)
		if block == nil {
			t.Fatalf("highlight.mjs has no %s member set", module)
		}
		listed := map[string]bool{}
		for _, quoted := range regexp.MustCompile(`"(\w+)"`).FindAllStringSubmatch(block[1], -1) {
			listed[quoted[1]] = true
		}
		if got, expected := strings.Join(sortedNames(listed), " "), strings.Join(sortedNames(members), " "); got != expected {
			t.Errorf("highlight.mjs %s members:\n got %s\nwant %s", module, got, expected)
		}
		grammarListed := map[string]bool{}
		for _, name := range strings.Split(grammarMembers[module], "|") {
			grammarListed[name] = true
		}
		if got, expected := strings.Join(sortedNames(grammarListed), " "), strings.Join(sortedNames(members), " "); got != expected {
			t.Errorf("tiger.tmLanguage.json %s members:\n got %s\nwant %s", module, got, expected)
		}
	}
}
