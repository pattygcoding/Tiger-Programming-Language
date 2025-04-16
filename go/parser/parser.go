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
	case token.PRINT:
		return p.parsePrintStatement()
	case token.IF:
		return p.parseIfStatement()
	case token.WHILE:
		return p.parseWhileStatement()
	case token.LBRACE:
		return p.parseBlockStatement()
	case token.FUNC:
		return p.parseFunctionDefinition()
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

func (p *Parser) parsePrintStatement() *ast.PrintStatement {
	p.nextToken()
	value := p.parseExpression()
	return &ast.PrintStatement{Value: value}
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
		left := p.parseExpression()
		op := p.curToken.Literal
		p.nextToken()
		right := p.parseExpression()
		p.nextToken() // skip RPAREN
		return &ast.InfixExpression{Left: left, Operator: op, Right: right}
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
