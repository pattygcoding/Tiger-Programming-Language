package compiler

import (
	"fmt"
	"tiger/pkg/ast"
	"tiger/pkg/evaluator"
	"tiger/pkg/parser"
)

func collectModules(source, filename string) (evaluator.BundleLoader, string, error) {
	loader := evaluator.FileLoader{}
	entry, err := loader.Resolve("", filename)
	bundle := evaluator.BundleLoader{Sources: map[string]string{}, Paths: map[string]map[string]string{}}
	if err != nil {
		return bundle, "", err
	}
	var visit func(string, string, int) error
	visit = func(filename, source string, depth int) error {
		if _, exists := bundle.Sources[filename]; exists {
			return nil
		}
		if depth > 128 {
			return fmt.Errorf("%s: maximum import depth exceeded", filename)
		}
		program, err := parser.ParseSource(source, filename)
		if err != nil {
			return err
		}
		bundle.Sources[filename] = source
		bundle.Paths[filename] = map[string]string{}
		for _, declaration := range ast.Imports(program) {
			dependency, err := loader.Resolve(filename, declaration.Path)
			if err != nil {
				return declaration.Position().Errorf("%v", err)
			}
			bundle.Paths[filename][declaration.Path] = dependency
			if _, exists := bundle.Sources[dependency]; exists {
				continue
			}
			content, err := loader.Load(dependency)
			if err != nil {
				return declaration.Position().Errorf("cannot import %q: %v", dependency, err)
			}
			if err := visit(dependency, content, depth+1); err != nil {
				return err
			}
		}
		return nil
	}
	err = visit(entry, source, 0)
	return bundle, entry, err
}
