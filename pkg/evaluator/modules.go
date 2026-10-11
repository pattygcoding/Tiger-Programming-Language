package evaluator

import (
	"io"
	"path"

	"tiger/pkg/ast"
	"tiger/pkg/format"
	"tiger/pkg/object"
	"tiger/pkg/parser"
)

type ModuleLoader interface {
	Resolve(importer, requested string) (string, error)
	Load(filename string) (string, error)
}

func RunWithLoader(source, filename string, loader ModuleLoader, output io.Writer) error {
	return RunWithOptions(source, filename, loader, nil, output)
}

// RunWithOptions runs source with both a module loader and a filesystem. A nil
// filesystem uses the host disk; browser builds and tests supply a memory
// filesystem instead.
func RunWithOptions(source, filename string, loader ModuleLoader, files FileSystem, output io.Writer) error {
	return RunWithInput(source, filename, loader, files, nil, output)
}

// RunWithInput is RunWithOptions plus a reader for input(); nil makes input() fail.
func RunWithInput(source, filename string, loader ModuleLoader, files FileSystem, input io.Reader, output io.Writer) error {
	program, err := parser.ParseSource(source, filename)
	if err != nil {
		return err
	}
	eval := New(output)
	eval.SourcePath, eval.Loader, eval.FileSystem, eval.Input = filename, loader, files, input
	return eval.Execute(program)
}

func (eval *Evaluator) importModule(node *ast.Import, env *object.Environment) *object.Module {
	if eval.Loader == nil {
		fail(node, "imports require a module loader; use RunFile or configure Evaluator.Loader")
	}
	if path.Ext(node.Path) != ".tg" {
		fail(node, "import path must end in .tg")
	}
	filename, err := eval.Loader.Resolve(env.ModulePath(), node.Path)
	check(node, err)
	if module := eval.modules[filename]; module != nil {
		return module
	}
	if eval.loading[filename] {
		fail(node, "circular import of %q", filename)
	}
	if eval.importDepth >= 128 {
		fail(node, "maximum import depth exceeded")
	}
	source, err := eval.Loader.Load(filename)
	if err != nil {
		fail(node, "cannot import %q: %v", filename, err)
	}
	program, err := parser.ParseSource(source, filename)
	check(node, err)
	eval.loading[filename] = true
	eval.importDepth++
	defer func() { delete(eval.loading, filename); eval.importDepth-- }()
	builtins := object.NewEnvironment(nil)
	eval.builtins(builtins)
	scope := object.NewEnvironment(builtins)
	scope.SourcePath = filename
	module := &object.Module{Name: filename, Env: scope}
	eval.block(program.Statements, scope)
	eval.modules[filename] = module
	return module
}

type BundleLoader struct {
	Sources map[string]string
	Paths   map[string]map[string]string
}

func (loader BundleLoader) Resolve(importer, requested string) (string, error) {
	if filename, exists := loader.Paths[importer][requested]; exists {
		return filename, nil
	}
	return "", format.Errorf("module %q imported from %q is not bundled", requested, importer)
}

func (loader BundleLoader) Load(filename string) (string, error) {
	if source, exists := loader.Sources[filename]; exists {
		return source, nil
	}
	return "", format.Errorf("module %q is not bundled", filename)
}
