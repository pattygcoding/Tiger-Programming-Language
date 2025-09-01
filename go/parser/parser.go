package parser

import (
	"strconv"
	"tiger/go/ast"
	"tiger/go/lexer"
	"tiger/go/token"
)

type Parser struct {
	l         *lexer.Lexer
	curToken  token.Token
	peekToken token.Token
}

func New(l *lexer.Lexer) *Parser {
	p := &Parser{l: l}
	p.nextToken()
	p.nextToken()
	return p
}

func (p *Parser) nextToken() {
	p.curToken = p.peekToken
	p.peekToken = p.l.NextToken()
}

func (p *Parser) ParseProgram() *ast.Program {
	program := &ast.Program{}
	for p.curToken.Type != token.EOF {
		stmt := p.parseStatement()
		if stmt != nil {
			program.Statements = append(program.Statements, stmt)
		}
		p.nextToken()
	}
	return program
}

func (p *Parser) parseStatement() ast.Statement {
	switch p.curToken.Type {
	case token.LET:
		return p.parseLetStatement()
	case token.CONST:
		return p.parseConstStatement()
	case token.IF:
		return p.parseIfStatement()
	case token.WHILE:
		return p.parseWhileStatement()
	case token.FOR:
		return p.parseForStatement()
	case token.LBRACE:
		return p.parseBlockStatement()
	case token.FUNC:
		return p.parseFunctionDefinition()
	case token.CLASS:
		return p.parseClassStatement()
	case token.RETURN:
		return p.parseReturnStatement()
	default:
		return p.parseExpressionStatement()
	}
}

func (p *Parser) parseLetStatement() *ast.LetStatement {
	p.nextToken()
	name := &ast.Identifier{Value: p.curToken.Literal}
	p.nextToken() // skip =
	p.nextToken() // value token
	value := p.parseExpression()
	return &ast.LetStatement{Name: name, Value: value}
}

func (p *Parser) parseIfStatement() *ast.IfStatement {
	p.nextToken() // skip 'if'
	condition := p.parseExpression()
	p.nextToken() // skip '{'
	consequence := p.parseBlockStatement()

	var alternative *ast.BlockStatement
	if p.peekToken.Type == token.ELSE {
		p.nextToken() // skip else
		p.nextToken() // skip {
		alternative = p.parseBlockStatement()
	}

	return &ast.IfStatement{
		Condition:   condition,
		Consequence: consequence,
		Alternative: alternative,
	}
}

func (p *Parser) parseWhileStatement() *ast.WhileStatement {
	p.nextToken() // skip 'while'
	condition := p.parseExpression()
	p.nextToken() // skip to {
	body := p.parseBlockStatement()
	return &ast.WhileStatement{
		Condition: condition,
		Body:      body,
	}
}

func (p *Parser) parseBlockStatement() *ast.BlockStatement {
	block := &ast.BlockStatement{}
	p.nextToken() // skip {
	for p.curToken.Type != token.RBRACE && p.curToken.Type != token.EOF {
		stmt := p.parseStatement()
		if stmt != nil {
			block.Statements = append(block.Statements, stmt)
		}
		p.nextToken()
	}
	return block
}

func (p *Parser) parseExpression() ast.Expression {
	left := p.parsePrimaryExpression()
	
	for p.peekToken.Type == token.PLUS || p.peekToken.Type == token.MINUS || 
		p.peekToken.Type == token.ASTERISK || p.peekToken.Type == token.SLASH ||
		p.peekToken.Type == token.EQ || p.peekToken.Type == token.NOT_EQ ||
		p.peekToken.Type == token.LT || p.peekToken.Type == token.GT ||
		p.peekToken.Type == token.LTE || p.peekToken.Type == token.GTE {
		p.nextToken()
		operator := p.curToken.Literal
		p.nextToken()
		right := p.parsePrimaryExpression()
		left = &ast.InfixExpression{Left: left, Operator: operator, Right: right}
	}
	
	return left
}

func (p *Parser) parsePrimaryExpression() ast.Expression {
	switch p.curToken.Type {
	case token.STRING:
		return &ast.StringLiteral{Value: p.curToken.Literal}
	case token.IDENT:
		ident := &ast.Identifier{Value: p.curToken.Literal}
		if p.peekToken.Type == token.LPAREN {
			return p.parseCallExpression(ident)
		}
		return ident
	case token.INT:
		val, _ := strconv.ParseInt(p.curToken.Literal, 0, 64)
		return &ast.IntegerLiteral{Value: val}
	case token.FLOAT:
		val, _ := strconv.ParseFloat(p.curToken.Literal, 64)
		return &ast.FloatLiteral{Value: val}
	case token.TRUE:
		return &ast.Boolean{Value: true}
	case token.FALSE:
		return &ast.Boolean{Value: false}
	case token.LPAREN:
		p.nextToken()
		exp := p.parseExpression()
		p.nextToken() // skip RPAREN
		return exp
	default:
		return nil
	}
}

func (p *Parser) parseFunctionDefinition() *ast.LetStatement {
	p.nextToken() // skip 'func'
	name := p.curToken.Literal
	p.nextToken() // skip name
	p.nextToken() // skip LPAREN

	params := []*ast.Identifier{}
	for p.curToken.Type != token.RPAREN && p.curToken.Type != token.EOF {
		params = append(params, &ast.Identifier{Value: p.curToken.Literal})
		p.nextToken()
		if p.curToken.Type == token.COMMA {
			p.nextToken() // skip comma
		}
	}
	p.nextToken() // skip RPAREN
	body := p.parseBlockStatement()
	return &ast.LetStatement{
		Name: &ast.Identifier{Value: name},
		Value: &ast.FunctionLiteral{
			Name:       name,
			Parameters: params,
			Body:       body,
		},
	}
}

func (p *Parser) parseCallExpression(fn ast.Expression) ast.Expression {
	p.nextToken() // skip LPAREN
	args := []ast.Expression{}
	for p.curToken.Type != token.RPAREN && p.curToken.Type != token.EOF {
		arg := p.parseExpression()
		args = append(args, arg)
		p.nextToken()
		if p.curToken.Type == token.COMMA {
			p.nextToken()
		}
	}
	return &ast.CallExpression{
		Function:  fn,
		Arguments: args,
	}
}

func (p *Parser) parseExpressionStatement() *ast.ExpressionStatement {
	expr := p.parseExpression()
	return &ast.ExpressionStatement{Expression: expr}
}

func (p *Parser) parseConstStatement() *ast.ConstStatement {
	p.nextToken()
	name := &ast.Identifier{Value: p.curToken.Literal}
	p.nextToken() // skip =
	p.nextToken() // value token
	value := p.parseExpression()
	return &ast.ConstStatement{Name: name, Value: value}
}

func (p *Parser) parseForStatement() *ast.ForStatement {
	p.nextToken() // skip 'for'
	p.nextToken() // skip '('
	
	// Parse init statement
	var init ast.Statement
	if p.curToken.Type == token.LET {
		init = p.parseLetStatement()
	}
	p.nextToken() // skip ';'
	p.nextToken()
	
	// Parse condition
	condition := p.parseExpression()
	p.nextToken() // skip ';'
	p.nextToken()
	
	// Parse update statement
	var update ast.Statement
	if p.curToken.Type == token.IDENT {
		update = p.parseExpressionStatement()
	}
	p.nextToken() // skip ')'
	p.nextToken() // skip '{'
	
	// Parse body
	body := p.parseBlockStatement()
	
	return &ast.ForStatement{
		Init:      init,
		Condition: condition,
		Update:    update,
		Body:      body,
	}
}

func (p *Parser) parseReturnStatement() *ast.ReturnStatement {
	p.nextToken()
	value := p.parseExpression()
	return &ast.ReturnStatement{Value: value}
}

func (p *Parser) parseClassStatement() *ast.ClassStatement {
	p.nextToken()
	name := &ast.Identifier{Value: p.curToken.Literal}
	p.nextToken() // skip '{'
	
	methods := []*ast.FunctionLiteral{}
	for p.curToken.Type != token.RBRACE && p.curToken.Type != token.EOF {
		if p.curToken.Type == token.FUNC {
			letStmt := p.parseFunctionDefinition()
			if functionLit, ok := letStmt.Value.(*ast.FunctionLiteral); ok {
				methods = append(methods, functionLit)
			}
		}
		p.nextToken()
	}
	
	return &ast.ClassStatement{
		Name:    name,
		Methods: methods,
	}
}
