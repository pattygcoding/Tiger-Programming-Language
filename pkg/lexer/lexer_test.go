package lexer

import (
	"strings"
	"testing"
)

func TestScan(t *testing.T) {
	tokens, err := Scan("# comment\nconst answer = 1.5e2; // comment\nprint(\"hi\\n\", 'there', true != false);")
	if err != nil {
		t.Fatal(err)
	}
	want := []Kind{"const", Ident, "=", Number, ";", Ident, "(", String, ",", String, ",", "true", "!=", "false", ")", ";", EOF}
	if len(tokens) != len(want) {
		t.Fatalf("tokens: %#v", tokens)
	}
	for index, kind := range want {
		if tokens[index].Kind != kind {
			t.Errorf("token %d: got %s, want %s", index, tokens[index].Kind, kind)
		}
	}
	if tokens[0].Line != 2 || tokens[0].Column != 1 || tokens[7].Text != "hi\n" {
		t.Fatalf("incorrect positions or escapes: %#v", tokens)
	}
}

func TestInvalidInput(t *testing.T) {
	for _, source := range []string{"@", "1e;", "\"unfinished", "\"bad\\q\"", "\"line\nbreak\"", "!true", "\x00"} {
		t.Run(source, func(t *testing.T) {
			if _, err := Scan(source); err == nil || !strings.Contains(err.Error(), "1:1:") {
				t.Fatalf("expected positioned error, got %v", err)
			}
		})
	}
}
