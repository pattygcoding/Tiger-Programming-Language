package evaluator

import (
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"strings"
	"tiger/pkg/ast"
	"tiger/pkg/object"
	"tiger/pkg/parser"
)

type ModuleLoader interface {
	Resolve(importer, requested string) (string, error)
	Load(filename string) (string, error)
}

type FileLoader struct{}

func (FileLoader) Resolve(importer, requested string) (string, error) {
	if filepath.Ext(requested) != ".tg" {
		return "", fmt.Errorf("import path must end in .tg")
	}
	filename := filepath.FromSlash(requested)
	if !filepath.IsAbs(filename) {
		filename = filepath.Join(filepath.Dir(importer), filename)
	}
	filename, err := filepath.Abs(filename)
	if err != nil {
		return "", err
	}
	if canonical, err := filepath.EvalSymlinks(filename); err == nil {
		filename = canonical
	}
	return filename, nil
}

func (FileLoader) Load(filename string) (string, error) {
	content, err := os.ReadFile(filename)
	return string(content), err
}

type FSLoader struct{ FS fs.FS }

func (FSLoader) Resolve(importer, requested string) (string, error) {
	if path.Ext(requested) != ".tg" || strings.Contains(requested, "\\") || path.IsAbs(requested) {
		return "", fmt.Errorf("import path must be a relative .tg path using forward slashes")
	}
	filename := path.Join(path.Dir(importer), requested)
	if !fs.ValidPath(filename) {
		return "", fmt.Errorf("import path escapes the module root")
	}
	return filename, nil
}

func (loader FSLoader) Load(filename string) (string, error) {
	content, err := fs.ReadFile(loader.FS, filename)
	return string(content), err
}

func RunFile(filename string, output io.Writer) error {
	loader := FileLoader{}
	canonical, err := loader.Resolve("", filename)
	if err != nil {
		return err
	}
	source, err := loader.Load(canonical)
	if err != nil {
		return err
	}
	return RunWithLoader(source, canonical, loader, output)
}

func RunWithLoader(source, filename string, loader ModuleLoader, output io.Writer) error {
	program, err := parser.ParseSource(source, filename)
	if err != nil {
		return err
	}
	eval := New(output)
	eval.SourcePath, eval.Loader = filename, loader
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
	return "", fmt.Errorf("module %q imported from %q is not bundled", requested, importer)
}

func (loader BundleLoader) Load(filename string) (string, error) {
	if source, exists := loader.Sources[filename]; exists {
		return source, nil
	}
	return "", fmt.Errorf("module %q is not bundled", filename)
}
