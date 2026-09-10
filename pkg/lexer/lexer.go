package lexer

import (
	"fmt"
	"strconv"
	"unicode"
)

type Kind string

const (
	EOF    Kind = "EOF"
	Ident  Kind = "identifier"
	Number Kind = "number"
	String Kind = "string"
)

type Token struct {
	Kind   Kind
	Text   string
	Line   int
	Column int
}

func (token Token) Errorf(format string, args ...any) error {
	return fmt.Errorf("%d:%d: %s", token.Line, token.Column, fmt.Sprintf(format, args...))
}

type scanner struct {
	source []rune
	pos    int
	line   int
	column int
}

func Scan(source string) ([]Token, error) {
	scan := scanner{source: []rune(source), line: 1, column: 1}
	tokens := []Token{}
	for scan.pos < len(scan.source) {
		current := scan.peek(0)
		if unicode.IsSpace(current) {
			scan.advance()
			continue
		}
		if current == '#' || current == '/' && scan.peek(1) == '/' {
			for scan.pos < len(scan.source) && scan.peek(0) != '\n' {
				scan.advance()
			}
			continue
		}
		token := Token{Line: scan.line, Column: scan.column}
		start := scan.pos
		switch {
		case unicode.IsLetter(current) || current == '_':
			for unicode.IsLetter(scan.peek(0)) || unicode.IsDigit(scan.peek(0)) || scan.peek(0) == '_' {
				scan.advance()
			}
			token.Text = string(scan.source[start:scan.pos])
			token.Kind = Ident
			switch token.Text {
			case "const", "def", "class", "super", "return", "if", "elif", "else", "while", "for", "in", "true", "false", "null", "and", "or", "not":
				token.Kind = Kind(token.Text)
			}
		case current >= '0' && current <= '9':
			for scan.peek(0) >= '0' && scan.peek(0) <= '9' {
				scan.advance()
			}
			if scan.peek(0) == '.' && scan.peek(1) >= '0' && scan.peek(1) <= '9' {
				scan.advance()
				for scan.peek(0) >= '0' && scan.peek(0) <= '9' {
					scan.advance()
				}
			}
			if scan.peek(0) == 'e' || scan.peek(0) == 'E' {
				scan.advance()
				if scan.peek(0) == '+' || scan.peek(0) == '-' {
					scan.advance()
				}
				for scan.peek(0) >= '0' && scan.peek(0) <= '9' {
					scan.advance()
				}
			}
			token.Text = string(scan.source[start:scan.pos])
			token.Kind = Number
			if _, err := strconv.ParseFloat(token.Text, 64); err != nil {
				return nil, token.Errorf("invalid number %q", token.Text)
			}
		case current == '"' || current == '\'':
			quote := scan.advance()
			value := []rune{}
			for scan.pos < len(scan.source) && scan.peek(0) != quote {
				char := scan.advance()
				if char == '\n' || char == '\r' {
					return nil, token.Errorf("unterminated string")
				}
				if char == '\\' {
					if scan.pos == len(scan.source) {
						return nil, token.Errorf("unterminated string")
					}
					char = scan.advance()
					switch char {
					case 'n':
						char = '\n'
					case 'r':
						char = '\r'
					case 't':
						char = '\t'
					case '\\', '"', '\'':
					default:
						return nil, token.Errorf("unknown escape \\%c", char)
					}
				}
				value = append(value, char)
			}
			if scan.pos == len(scan.source) {
				return nil, token.Errorf("unterminated string")
			}
			scan.advance()
			token.Kind, token.Text = String, string(value)
		default:
			scan.advance()
			token.Text = string(current)
			switch current {
			case '=', '!', '<', '>':
				if scan.peek(0) == '=' {
					token.Text += string(scan.advance())
				} else if current == '!' {
					return nil, token.Errorf("expected '=' after '!'; use 'not' for negation")
				}
			case '+', '-', '*', '/', '%', '(', ')', '[', ']', '{', '}', ':', ',', ';', '.':
			default:
				return nil, token.Errorf("unexpected character %q", current)
			}
			token.Kind = Kind(token.Text)
		}
		tokens = append(tokens, token)
	}
	return append(tokens, Token{Kind: EOF, Line: scan.line, Column: scan.column}), nil
}

func (scan *scanner) peek(offset int) rune {
	if scan.pos+offset >= len(scan.source) {
		return 0
	}
	return scan.source[scan.pos+offset]
}

func (scan *scanner) advance() rune {
	char := scan.source[scan.pos]
	scan.pos++
	if char == '\n' {
		scan.line++
		scan.column = 1
	} else {
		scan.column++
	}
	return char
}
