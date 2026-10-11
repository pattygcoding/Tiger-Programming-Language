package evaluator

import (
	"tiger/pkg/ast"
	"tiger/pkg/object"
)

// destructure binds several names from a list or dictionary value.
func (eval *Evaluator) destructure(node *ast.Destructure, env *object.Environment) *flow {
	value := eval.expression(node.Value, env)
	if node.Lookup {
		record, ok := value.(*object.Dict)
		if !ok {
			fail(node, "cannot destructure %s with { ... }; expected a dictionary", value.Type())
		}
		for _, name := range node.Names {
			entry, exists, err := record.Get(object.String(name))
			check(node, err)
			if !exists {
				fail(node, "dictionary is missing key %q", name)
			}
			check(node, env.Define(name, entry, node.Constant))
		}
		return nil
	}
	list, ok := value.(*object.List)
	if !ok {
		fail(node, "cannot destructure %s with [ ... ]; expected a list", value.Type())
	}
	if len(list.Elements) != len(node.Names) {
		fail(node, "cannot unpack %d values into %d names", len(list.Elements), len(node.Names))
	}
	for index, name := range node.Names {
		check(node, env.Define(name, list.Elements[index], node.Constant))
	}
	return nil
}
