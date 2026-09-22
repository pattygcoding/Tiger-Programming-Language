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
	loopDepth     int
	switchDepth   int
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
	case parse.match("throw"):
		value := parse.expression(0)
		parse.expect(";")
		return &ast.Throw{Base: base, Value: value}
	case parse.match("try"):
		statement := &ast.Try{Base: base, Body: parse.block()}
		parse.expect("catch")
		parse.expect("(")
		statement.Name = parse.expect(lexer.Ident).Text
		parse.expect(")")
		statement.Catch = parse.block()
		return statement
	case parse.match("class"):
		return parse.class(base)
	case parse.match("function"):
		name := parse.expect(lexer.Ident).Text
		parse.expect("(")
		parameters := []string{}
		seen := map[string]bool{}
		if !parse.at(")") {
			for {
				parameter := parse.current()
				if parse.at("this") {
					parse.take()
				} else {
					parse.expect(lexer.Ident)
				}
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
		loopDepth, switchDepth := parse.loopDepth, parse.switchDepth
		parse.loopDepth, parse.switchDepth = 0, 0
		body := parse.block()
		parse.loopDepth, parse.switchDepth = loopDepth, switchDepth
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
		body, otherwise := parse.loopBody()
		return &ast.While{Base: base, Condition: condition, Body: body, Else: otherwise}
	case parse.match("for"):
		name := parse.expect(lexer.Ident).Text
		parse.expect("in")
		iterable := parse.expression(0)
		body, otherwise := parse.loopBody()
		return &ast.For{Base: base, Name: name, Iterable: iterable, Body: body, Else: otherwise}
	case parse.match("cfor"):
		loop := &ast.CFor{Base: base}
		parse.expect("(")
		if !parse.at(";") {
			loop.Initializer = parse.simpleStatement()
		}
		parse.expect(";")
		if !parse.at(";") {
			loop.Condition = parse.expression(0)
		}
		parse.expect(";")
		if !parse.at(")") {
			loop.Update = parse.simpleStatement()
			if assignment, ok := loop.Update.(*ast.Assign); ok && assignment.Declaration {
				panic(syntaxError{assignment.Position().Errorf("cfor update cannot declare a variable")})
			}
		}
		parse.expect(")")
		loop.Body, loop.Else = parse.loopBody()
		return loop
	case parse.at("break") || parse.at("continue"):
		kind := parse.take().Text
		if parse.loopDepth == 0 && (kind == "continue" || parse.switchDepth == 0) {
			panic(syntaxError{base.Token.Errorf("%s outside a loop%s", kind, map[bool]string{true: " or switch"}[kind == "break"])})
		}
		parse.expect(";")
		return &ast.Control{Base: base, Kind: kind}
	case parse.match("switch"):
		parse.enter()
		defer func() { parse.depth-- }()
		statement := &ast.Switch{Base: base, Value: parse.expression(0)}
		parse.expect("{")
		parse.switchDepth++
		seenDefault := false
		for !parse.at("}") && !parse.at(lexer.EOF) {
			branch := ast.Case{}
			if parse.match("case") {
				branch.Value = parse.expression(0)
			} else {
				marker := parse.expect("default")
				if seenDefault {
					panic(syntaxError{marker.Errorf("duplicate default case")})
				}
				seenDefault = true
			}
			parse.expect(":")
			for !parse.at("case") && !parse.at("default") && !parse.at("}") && !parse.at(lexer.EOF) {
				branch.Body = append(branch.Body, parse.statement())
			}
			statement.Cases = append(statement.Cases, branch)
		}
		parse.switchDepth--
		parse.expect("}")
		parse.match(";")
		return statement
	default:
		statement := parse.simpleStatement()
		parse.expect(";")
		return statement
	}
}

func (parse *parser) simpleStatement() ast.Stmt {
	base := ast.Base{Token: parse.current()}
	if parse.at("const") || parse.at("var") {
		constant := parse.take().Kind == "const"
		name := parse.expect(lexer.Ident)
		parse.expect("=")
		return &ast.Assign{Base: base, Target: &ast.Identifier{Base: ast.Base{Token: name}, Name: name.Text}, Value: parse.expression(0), Constant: constant, Declaration: true}
	}
	value := parse.expression(0)
	if parse.match("=") {
		parse.assignmentTarget(value)
		return &ast.Assign{Base: base, Target: value, Value: parse.expression(0)}
	}
	return &ast.ExpressionStmt{Base: base, Value: value}
}

func (parse *parser) assignmentTarget(value ast.Expr) {
	switch value.(type) {
	case *ast.Identifier, *ast.Index, *ast.Property:
	default:
		panic(syntaxError{value.Position().Errorf("invalid assignment target")})
	}
}

func (parse *parser) loopBody() ([]ast.Stmt, []ast.Stmt) {
	parse.loopDepth++
	body := parse.block()
	parse.loopDepth--
	var otherwise []ast.Stmt
	if parse.match("else") {
		otherwise = parse.block()
	}
	return body, otherwise
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
		access := "public"
		if parse.at("public") || parse.at("private") || parse.at("protected") {
			access = parse.take().Text
		}
		if parse.at("var") || parse.at("const") {
			field := parse.statement().(*ast.Assign)
			name := field.Target.(*ast.Identifier).Name
			if seen[name] {
				panic(syntaxError{field.Position().Errorf("duplicate member %q", name)})
			}
			seen[name] = true
			declaration.Fields = append(declaration.Fields, &ast.Field{Base: field.Base, Name: name, Access: access, Constant: field.Constant, Value: field.Value})
			continue
		}
		if !parse.at("function") {
			panic(syntaxError{parse.current().Errorf("class bodies may only contain methods and field declarations")})
		}
		method := parse.statement().(*ast.Function)
		method.Access = access
		if len(method.Parameters) == 0 || method.Parameters[0] != "this" {
			panic(syntaxError{method.Position().Errorf("method %q must declare this as its first parameter", method.Name)})
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
	case "++", "--":
		return 8
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
	case lexer.FString:
		formatted := &ast.FormattedString{Base: base}
		for _, part := range token.Parts {
			if part.Kind == lexer.String {
				formatted.Parts = append(formatted.Parts, &ast.Literal{Base: ast.Base{Token: part}, Value: part.Text})
			} else {
				nested := parser{tokens: part.Parts, depth: parse.depth}
				formatted.Parts = append(formatted.Parts, nested.expression(0))
				nested.expect(lexer.EOF)
			}
		}
		left = formatted
	case "true", "false":
		left = &ast.Literal{Base: base, Value: token.Kind == "true"}
	case "null":
		left = &ast.Literal{Base: base}
	case lexer.Ident, "this":
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
	case "++", "--":
		target := parse.expression(6)
		parse.assignmentTarget(target)
		left = &ast.Update{Base: base, Target: target, Operator: token.Text, Prefix: true}
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
		case "++", "--":
			parse.assignmentTarget(left)
			left = &ast.Update{Base: base, Target: left, Operator: operator.Text}
		case ".":
			member := parse.current()
			if parse.at("this") {
				parse.take()
			} else {
				parse.expect(lexer.Ident)
			}
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
