package evaluator

import (
	"tiger/pkg/ast"
	"tiger/pkg/object"
)

// tryStatement evaluates a try/catch/finally statement. A finally block runs on
// every exit path: normal completion, a thrown value, a runtime error, or a
// return/break/continue. A control-flow statement inside finally overrides any
// in-flight error, and an error raised by finally replaces the original.
func (eval *Evaluator) tryStatement(node *ast.Try, env *object.Environment) *flow {
	if len(node.Finally) == 0 {
		return eval.tryHandlers(node, env)
	}
	var result *flow
	var failure any
	func() {
		defer func() { failure = recover() }()
		result = eval.tryHandlers(node, env)
	}()
	if fatal, ok := failure.(runtimeError); ok && fatal.fatal {
		panic(failure) // resource limits bypass finally
	}
	if final := eval.block(node.Finally, object.NewEnvironment(env)); final != nil {
		return final
	}
	if failure != nil {
		panic(failure)
	}
	return result
}

// tryHandlers runs the body and then each catch clause in order. A clause
// matches when it has no condition or its condition is truthy; the first match
// wins. An error no clause matches propagates unchanged.
func (eval *Evaluator) tryHandlers(node *ast.Try, env *object.Environment) *flow {
	var caught object.Value
	var failure any
	var result *flow
	func() {
		defer func() {
			if recovered := recover(); recovered != nil {
				switch recovered := recovered.(type) {
				case thrown:
					failure, caught = recovered, recovered.value
				case runtimeError:
					if recovered.fatal {
						panic(recovered)
					}
					failure, caught = recovered, recovered.value
				default:
					panic(recovered)
				}
			}
		}()
		result = eval.block(node.Body, object.NewEnvironment(env))
	}()
	if caught == nil {
		return result
	}
	for _, clause := range node.Catches {
		scope := object.NewEnvironment(env)
		check(node, scope.Define(clause.Name, caught, false))
		if clause.Condition != nil && !object.Truthy(eval.expression(clause.Condition, scope)) {
			continue
		}
		return eval.block(clause.Body, scope)
	}
	panic(failure)
}
