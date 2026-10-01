package ast_test

import (
	"testing"
	"tiger/pkg/ast"
	"tiger/pkg/parser"
)

func TestImportsInsideConstructor(t *testing.T) {
	program, err := parser.Parse(`class Box { Box() { import "a.tg" as a; } function run() { import "b.tg" as b; } }`)
	if err != nil {
		t.Fatal(err)
	}
	imports := ast.Imports(program)
	if len(imports) != 2 || imports[0].Path != "a.tg" || imports[1].Path != "b.tg" {
		t.Fatalf("got %#v", imports)
	}
}
