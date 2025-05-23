package eval

import (
	"fmt"
	"tiger/go/ast"
)

type Environment struct {
	store map[string]interface{}
	outer *Environment
}

func NewEnvironment() *Environment {
	return &Environment{store: make(map[string]interface{})}
}

func (e *Environment) Set(name string, val interface{}) {
	e.store[name] = val
}

func (e *Environment) Get(name string) (interface{}, bool) {
	val, ok := e.store[name]
	if !ok && e.outer != nil {
		return e.outer.Get(name)
	}
	return val, ok
}

func Eval(node ast.Node, env *Environment) string {
	switch node := node.(type) {
	case *ast.Program:
		var output string
		for _, stmt := range node.Statements {
			out := Eval(stmt, env)
			if out != "" {
				output += out + "\n"
			}
		}
		return output

	case *ast.LetStatement:
		switch val := node.Value.(type) {
		case *ast.StringLiteral:
			env.Set(node.Name.Value, val.Value)
		case *ast.IntegerLiteral:
			env.Set(node.Name.Value, fmt.Sprintf("%d", val.Value))
		case *ast.FloatLiteral:
			env.Set(node.Name.Value, fmt.Sprintf("%.1f", val.Value))
		case *ast.Boolean:
			env.Set(node.Name.Value, fmt.Sprintf("%t", val.Value))
		case *ast.Identifier:
			if v, ok := env.Get(val.Value); ok {
				env.Set(node.Name.Value, v)
			}
		case *ast.FunctionLiteral:
			env.Set(node.Name.Value, val)
		}

	case *ast.PrintStatement:
		return evalExpression(node.Value, env)

	case *ast.IfStatement:
		if evalCondition(node.Condition, env) {
			return Eval(node.Consequence, env)
		} else if node.Alternative != nil {
			return Eval(node.Alternative, env)
		}

	case *ast.WhileStatement:
		var output string
		for evalCondition(node.Condition, env) {
			out := Eval(node.Body, env)
			if out != "" {
				output += out + "\n"
			}
		}
		return output

	case *ast.BlockStatement:
		var output string
		for _, stmt := range node.Statements {
			out := Eval(stmt, env)
			if out != "" {
				output += out + "\n"
			}
		}
		return output

	case *ast.ExpressionStatement:
		result := evalExpression(node.Expression, env)
		if result != "" {
			fmt.Println(result)
		}
	}
	return ""
}

func evalExpression(expr ast.Expression, env *Environment) string {
	switch val := expr.(type) {
	case *ast.Identifier:
		if v, ok := env.Get(val.Value); ok {
			if str, isString := v.(string); isString {
				return str
			}
			return fmt.Sprintf("%v", v)
		}
		return "[undefined variable: " + val.Value + "]"
	case *ast.StringLiteral:
		return val.Value
	case *ast.IntegerLiteral:
		return fmt.Sprintf("%d", val.Value)
	case *ast.FloatLiteral:
		return fmt.Sprintf("%.1f", val.Value)
	case *ast.Boolean:
		return fmt.Sprintf("%t", val.Value)
	case *ast.CallExpression:
		return evalCallExpression(val, env)
	}
	return ""
}

func evalCondition(node ast.Expression, env *Environment) bool {
	switch val := node.(type) {
	case *ast.Boolean:
		return val.Value
	case *ast.Identifier:
		if str, ok := env.Get(val.Value); ok {
			return str == "true"
		}
	case *ast.InfixExpression:
		left := evalExpression(val.Left, env)
		right := evalExpression(val.Right, env)
		switch val.Operator {
		case "==":
			return left == right
		case "!=":
			return left != right
		case "<":
			return left < right
		case "<=":
			return left <= right
		case ">":
			return left > right
		case ">=":
			return left >= right
		}
	}
	return false
}

func evalCallExpression(call *ast.CallExpression, env *Environment) string {
	fnName, ok := call.Function.(*ast.Identifier)
	if !ok {
		return "[error: unsupported function call]"
	}
	val, ok := env.Get(fnName.Value)
	if !ok {
		return "[undefined function: " + fnName.Value + "]"
	}
	fn, ok := val.(*ast.FunctionLiteral)
	if !ok {
		return "[error: not a function: " + fnName.Value + "]"
	}

	newEnv := &Environment{store: make(map[string]interface{}), outer: env}

	for i, param := range fn.Parameters {
		if i < len(call.Arguments) {
			arg := evalExpression(call.Arguments[i], env)
			newEnv.Set(param.Value, arg)
		}
	}

	bodyOutput := Eval(fn.Body, newEnv)
	if bodyOutput != "" {
		fmt.Print(bodyOutput)
	}

	return ""
}
