package docs_test

import (
	"bufio"
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"tiger/pkg/evaluator"
)

type fence struct {
	language string
	content  string
	line     int
}

func documents(t *testing.T) []string {
	t.Helper()
	paths, err := filepath.Glob("*.md")
	if err != nil {
		t.Fatal(err)
	}
	return append([]string{"../README.md"}, paths...)
}

func fences(t *testing.T, path string, content []byte) []fence {
	t.Helper()
	scanner := bufio.NewScanner(bytes.NewReader(content))
	var blocks []fence
	var current *fence
	line := 0
	for scanner.Scan() {
		line++
		text := scanner.Text()
		if current == nil {
			if strings.HasPrefix(text, "```") {
				current = &fence{language: strings.TrimSpace(strings.TrimPrefix(text, "```")), line: line}
			}
		} else if text == "```" {
			blocks = append(blocks, *current)
			current = nil
		} else {
			current.content += text + "\n"
		}
	}
	if err := scanner.Err(); err != nil {
		t.Fatal(err)
	}
	if current != nil {
		t.Fatalf("%s:%d: unclosed code fence", path, current.line)
	}
	return blocks
}

func TestDocumentationExamples(t *testing.T) {
	count := 0
	for _, path := range documents(t) {
		content, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		blocks := fences(t, path, content)
		for index, block := range blocks {
			if block.language != "tg" {
				continue
			}
			count++
			t.Run(fmt.Sprintf("%s/line_%d", filepath.Base(path), block.line), func(t *testing.T) {
				if index+1 == len(blocks) || blocks[index+1].language != "text" && blocks[index+1].language != "error" {
					t.Fatal("each Tiger example must be followed by a text output or error expectation")
				}
				expected := blocks[index+1]
				var output bytes.Buffer
				err := evaluator.Run(block.content, &output)
				if expected.language == "error" {
					message := strings.TrimSpace(expected.content)
					if message == "" || err == nil || !strings.Contains(err.Error(), message) {
						t.Fatalf("expected error containing %q, got %v", message, err)
					}
				} else {
					if err != nil {
						t.Fatal(err)
					}
					if output.String() != expected.content {
						t.Fatalf("output mismatch\nwant: %q\ngot:  %q", expected.content, output.String())
					}
				}
			})
		}
	}
	if count == 0 {
		t.Fatal("no Tiger documentation examples found")
	}
	t.Logf("checked %d Tiger documentation examples", count)
}

func TestDocumentationLinks(t *testing.T) {
	pattern := regexp.MustCompile(`\[[^\]]+\]\(([^)]+)\)`)
	for _, path := range documents(t) {
		content, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		var prose strings.Builder
		inCode := false
		for _, line := range strings.Split(string(content), "\n") {
			if strings.HasPrefix(line, "```") {
				inCode = !inCode
			} else if !inCode {
				prose.WriteString(line + "\n")
			}
		}
		for _, match := range pattern.FindAllStringSubmatch(prose.String(), -1) {
			target := strings.SplitN(match[1], "#", 2)[0]
			if target == "" || strings.Contains(target, "://") {
				continue
			}
			if _, err := os.Stat(filepath.Join(filepath.Dir(path), filepath.FromSlash(target))); err != nil {
				t.Errorf("%s: broken link %q: %v", path, target, err)
			}
		}
	}
}
