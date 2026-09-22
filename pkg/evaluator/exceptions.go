package evaluator

import (
	"tiger/pkg/ast"
	"tiger/pkg/object"
)

func (eval *Evaluator) tryStatement(node *ast.Try, env *object.Environment) *flow {
	var caught object.Value
	var result *flow
	func() {
		defer func() {
			if failure := recover(); failure != nil {
				switch failure := failure.(type) {
				case thrown:
					caught = failure.value
				case runtimeError:
					if failure.fatal {
						panic(failure)
					}
					caught = object.String(failure.err.Error())
				default:
					panic(failure)
				}
			}
		}()
		result = eval.block(node.Body, object.NewEnvironment(env))
	}()
	if caught != nil {
		scope := object.NewEnvironment(env)
		check(node, scope.Define(node.Name, caught, false))
		return eval.block(node.Catch, scope)
	}
	return result
}
