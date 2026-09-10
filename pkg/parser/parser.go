package parser

import (
	"strconv"
	"tiger/pkg/ast"
	"tiger/pkg/lexer"
)

type syntaxError struct{ err error }

type parser struct {
	tokens        []lexer.Token
	pos           int
	functionDepth int
	depth         int
}

func Parse(source string) (program *ast.Program, err error) {
	tokens, err := lexer.Scan(source)
	if err != nil {
		return nil, err
	}
	defer func() {
		if failure := recover(); failure != nil {
			if syntax, ok := failure.(syntaxError); ok {
				program, err = nil, syntax.err
			} else {
				panic(failure)
			}
		}
	}()
	parse := parser{tokens: tokens}
	program = &ast.Program{}
	for !parse.at(lexer.EOF) {
		program.Statements = append(program.Statements, parse.statement())
	}
	return program, nil
}

func (parse *parser) current() lexer.Token    { return parse.tokens[parse.pos] }
func (parse *parser) at(kind lexer.Kind) bool { return parse.current().Kind == kind }

func (parse *parser) take() lexer.Token {
	token := parse.current()
	if token.Kind != lexer.EOF {
		parse.pos++
	}
	return token
}

func (parse *parser) match(kind lexer.Kind) bool {
	if !parse.at(kind) {
		return false
	}
	parse.take()
	return true
}

func (parse *parser) expect(kind lexer.Kind) lexer.Token {
	if !parse.at(kind) {
		panic(syntaxError{parse.current().Errorf("expected %q, found %q", kind, parse.current().Kind)})
	}
	return parse.take()
}

func (parse *parser) enter() {
	parse.depth++
	if parse.depth > 512 {
		panic(syntaxError{parse.current().Errorf("maximum syntax nesting exceeded")})
	}
}

func (parse *parser) statement() ast.Stmt {
	base := ast.Base{Token: parse.current()}
	switch {
	case parse.match("class"):
		return parse.class(base)
	case parse.match("const"):
		name := parse.expect(lexer.Ident)
		parse.expect("=")
		value := parse.expression(0)
		parse.expect(";")
		return &ast.Assign{Base: base, Target: &ast.Identifier{Base: ast.Base{Token: name}, Name: name.Text}, Value: value, Constant: true}
	case parse.match("def"):
		name := parse.expect(lexer.Ident).Text
		parse.expect("(")
		parameters := []string{}
		seen := map[string]bool{}
		if !parse.at(")") {
			for {
				parameter := parse.expect(lexer.Ident)
				if seen[parameter.Text] {
					panic(syntaxError{parameter.Errorf("duplicate parameter %q", parameter.Text)})
				}
				seen[parameter.Text] = true
				parameters = append(parameters, parameter.Text)
				if !parse.match(",") || parse.at(")") {
					break
				}
			}
		}
		parse.expect(")")
		parse.functionDepth++
		body := parse.block()
		parse.functionDepth--
		return &ast.Function{Base: base, Name: name, Parameters: parameters, Body: body}
	case parse.match("return"):
		if parse.functionDepth == 0 {
			panic(syntaxError{base.Token.Errorf("return outside a function")})
		}
		var value ast.Expr
		if !parse.at(";") {
			value = parse.expression(0)
		}
		parse.expect(";")
		return &ast.Return{Base: base, Value: value}
	case parse.match("if"):
		conditional := &ast.If{Base: base}
		for {
			condition := parse.expression(0)
			conditional.Branches = append(conditional.Branches, ast.Branch{Condition: condition, Body: parse.block()})
			if !parse.match("elif") {
				break
			}
		}
		if parse.match("else") {
			conditional.Else = parse.block()
		}
		return conditional
	case parse.match("while"):
		condition := parse.expression(0)
		return &ast.While{Base: base, Condition: condition, Body: parse.block()}
	case parse.match("for"):
		name := parse.expect(lexer.Ident).Text
		parse.expect("in")
		iterable := parse.expression(0)
		return &ast.For{Base: base, Name: name, Iterable: iterable, Body: parse.block()}
	default:
		value := parse.expression(0)
		if parse.match("=") {
			switch value.(type) {
			case *ast.Identifier, *ast.Index, *ast.Property:
			default:
				panic(syntaxError{value.Position().Errorf("invalid assignment target")})
			}
			right := parse.expression(0)
			parse.expect(";")
			return &ast.Assign{Base: base, Target: value, Value: right}
		}
		parse.expect(";")
		return &ast.ExpressionStmt{Base: base, Value: value}
	}
}

func (parse *parser) class(base ast.Base) ast.Stmt {
	parse.enter()
	defer func() { parse.depth-- }()
	name := parse.expect(lexer.Ident)
	declaration := &ast.Class{Base: base, Name: name.Text}
	if parse.match("(") {
		parent := parse.expect(lexer.Ident)
		if parent.Text == name.Text {
			panic(syntaxError{parent.Errorf("a class cannot inherit from itself")})
		}
		declaration.Parent = &ast.Identifier{Base: ast.Base{Token: parent}, Name: parent.Text}
		parse.expect(")")
	}
	parse.expect("{")
	seen := map[string]bool{}
	for !parse.at("}") && !parse.at(lexer.EOF) {
		if !parse.at("def") {
			panic(syntaxError{parse.current().Errorf("class bodies may only contain method definitions")})
		}
		method := parse.statement().(*ast.Function)
		if len(method.Parameters) == 0 || method.Parameters[0] != "self" {
			panic(syntaxError{method.Position().Errorf("method %q must declare self as its first parameter", method.Name)})
		}
		if seen[method.Name] {
			panic(syntaxError{method.Position().Errorf("duplicate method %q", method.Name)})
		}
		seen[method.Name] = true
		declaration.Methods = append(declaration.Methods, method)
	}
	parse.expect("}")
	parse.match(";")
	return declaration
}

func (parse *parser) block() []ast.Stmt {
	parse.enter()
	defer func() { parse.depth-- }()
	parse.expect("{")
	statements := []ast.Stmt{}
	for !parse.at("}") && !parse.at(lexer.EOF) {
		statements = append(statements, parse.statement())
	}
	parse.expect("}")
	parse.match(";")
	return statements
}

func precedence(kind lexer.Kind) int {
	switch kind {
	case "or":
		return 1
	case "and":
		return 2
	case "==", "!=", "<", "<=", ">", ">=", "in":
		return 3
	case "+", "-":
		return 4
	case "*", "/", "%":
		return 5
	case "(", "[", ".":
		return 7
	}
	return 0
}

func (parse *parser) expression(minimum int) ast.Expr {
	parse.enter()
	defer func() { parse.depth-- }()
	token := parse.take()
	base := ast.Base{Token: token}
	var left ast.Expr
	switch token.Kind {
	case lexer.Number:
		value, _ := strconv.ParseFloat(token.Text, 64)
		left = &ast.Literal{Base: base, Value: value}
	case lexer.String:
		left = &ast.Literal{Base: base, Value: token.Text}
	case "true", "false":
		left = &ast.Literal{Base: base, Value: token.Kind == "true"}
	case "null":
		left = &ast.Literal{Base: base}
	case lexer.Ident:
		left = &ast.Identifier{Base: base, Name: token.Text}
	case "super":
		parse.expect(".")
		member := parse.expect(lexer.Ident)
		left = &ast.Property{Base: ast.Base{Token: member}, Receiver: &ast.Super{Base: base}, Name: member.Text}
	case "-", "+", "not":
		binding := 6
		if token.Kind == "not" {
			binding = 2
		}
		left = &ast.Unary{Base: base, Operator: token.Text, Right: parse.expression(binding)}
	case "(":
		left = parse.expression(0)
		parse.expect(")")
	case "[":
		left = &ast.List{Base: base, Elements: parse.expressions("]")}
	case "{":
		pairs := []ast.Pair{}
		if !parse.at("}") {
			for {
				key := parse.expression(0)
				parse.expect(":")
				pairs = append(pairs, ast.Pair{Key: key, Value: parse.expression(0)})
				if !parse.match(",") || parse.at("}") {
					break
				}
			}
		}
		parse.expect("}")
		left = &ast.Dict{Base: base, Pairs: pairs}
	default:
		panic(syntaxError{token.Errorf("expected expression, found %q", token.Kind)})
	}
	for precedence(parse.current().Kind) > minimum {
		operator := parse.take()
		base = ast.Base{Token: operator}
		switch operator.Kind {
		case ".":
			member := parse.expect(lexer.Ident)
			left = &ast.Property{Base: ast.Base{Token: member}, Receiver: left, Name: member.Text}
		case "(":
			left = &ast.Call{Base: base, Function: left, Arguments: parse.expressions(")")}
		case "[":
			left = &ast.Index{Base: base, Collection: left, Key: parse.expression(0)}
			parse.expect("]")
		default:
			left = &ast.Binary{Base: base, Left: left, Operator: operator.Text, Right: parse.expression(precedence(operator.Kind))}
		}
	}
	return left
}

func (parse *parser) expressions(end lexer.Kind) []ast.Expr {
	values := []ast.Expr{}
	if !parse.at(end) {
		for {
			values = append(values, parse.expression(0))
			if !parse.match(",") || parse.at(end) {
				break
			}
		}
	}
	parse.expect(end)
	return values
}
