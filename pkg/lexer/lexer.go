package lexer

import (
	"fmt"
	"strconv"
	"unicode"
)

type Kind string

const (
	EOF           Kind = "EOF"
	Ident         Kind = "identifier"
	Number        Kind = "number"
	String        Kind = "string"
	FString       Kind = "formatted string"
	Interpolation Kind = "interpolation"
)

type Token struct {
	Kind   Kind
	Text   string
	Line   int
	Column int
	Parts  []Token
	Source string
}

func (token Token) Errorf(format string, args ...any) error {
	if token.Source != "" {
		return fmt.Errorf("%s:%d:%d: %s", token.Source, token.Line, token.Column, fmt.Sprintf(format, args...))
	}
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
	return scan.tokens(false, 0)
}

func (scan *scanner) tokens(interpolation bool, depth int) ([]Token, error) {
	tokens := []Token{}
	braces := 0
	for scan.pos < len(scan.source) {
		current := scan.peek(0)
		if interpolation && current == '}' && braces == 0 {
			tokens = append(tokens, Token{Kind: EOF, Line: scan.line, Column: scan.column})
			scan.advance()
			return tokens, nil
		}
		if unicode.IsSpace(current) {
			scan.advance()
			continue
		}
		if current == '/' && scan.peek(1) == '/' {
			for scan.pos < len(scan.source) && scan.peek(0) != '\n' {
				scan.advance()
			}
			continue
		}
		token := Token{Line: scan.line, Column: scan.column}
		if current == '/' && scan.peek(1) == '*' {
			scan.advance()
			scan.advance()
			for scan.pos < len(scan.source) && !(scan.peek(0) == '*' && scan.peek(1) == '/') {
				scan.advance()
			}
			if scan.pos == len(scan.source) {
				return nil, token.Errorf("unterminated block comment")
			}
			scan.advance()
			scan.advance()
			continue
		}
		start := scan.pos
		switch {
		case (current == 'f' || current == 'F') && (scan.peek(1) == '"' || scan.peek(1) == '\''):
			if depth >= 512 {
				return nil, token.Errorf("maximum formatted string nesting exceeded")
			}
			formatted, err := scan.formattedString(token, depth+1)
			if err != nil {
				return nil, err
			}
			token = formatted
		case unicode.IsLetter(current) || current == '_':
			for unicode.IsLetter(scan.peek(0)) || unicode.IsDigit(scan.peek(0)) || scan.peek(0) == '_' {
				scan.advance()
			}
			token.Text = string(scan.source[start:scan.pos])
			token.Kind = Ident
			switch token.Text {
			case "const", "var", "function", "class", "super", "this", "public", "private", "protected", "return", "if", "elif", "else", "while", "for", "cfor", "in", "true", "false", "null", "and", "or", "not", "break", "continue", "switch", "case", "default", "try", "catch", "throw", "import", "as":
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
			case '+', '-':
				if scan.peek(0) == current || scan.peek(0) == '=' {
					token.Text += string(scan.advance())
				}
			case '*', '%':
				if scan.peek(0) == '=' {
					token.Text += string(scan.advance())
				}
			case '/', '(', ')', '[', ']', '{', '}', ':', ',', ';', '.':
			default:
				return nil, token.Errorf("unexpected character %q", current)
			}
			token.Kind = Kind(token.Text)
			if current == '{' {
				braces++
			}
			if current == '}' {
				braces--
			}
		}
		tokens = append(tokens, token)
	}
	if interpolation {
		return nil, (Token{Line: scan.line, Column: scan.column}).Errorf("unterminated f-string interpolation")
	}
	return append(tokens, Token{Kind: EOF, Line: scan.line, Column: scan.column}), nil
}

func (scan *scanner) formattedString(token Token, depth int) (Token, error) {
	scan.advance()
	quote := scan.advance()
	token.Kind = FString
	text := []rune{}
	flush := func() {
		if len(text) != 0 {
			token.Parts = append(token.Parts, Token{Kind: String, Text: string(text), Line: token.Line, Column: token.Column})
			text = nil
		}
	}
	for scan.pos < len(scan.source) {
		char := scan.advance()
		if char == quote {
			flush()
			return token, nil
		}
		if char == '\n' || char == '\r' {
			return token, token.Errorf("unterminated formatted string")
		}
		if char == '{' || char == '}' {
			if scan.peek(0) == char {
				scan.advance()
				text = append(text, char)
				continue
			}
			if char == '}' {
				return token, token.Errorf("single '}' in formatted string; use '}}' for a literal brace")
			}
			flush()
			part := Token{Kind: Interpolation, Line: scan.line, Column: scan.column}
			parts, err := scan.tokens(true, depth)
			if err != nil {
				return token, err
			}
			part.Parts = parts
			token.Parts = append(token.Parts, part)
			continue
		}
		if char == '\\' {
			if scan.pos == len(scan.source) {
				return token, token.Errorf("unterminated formatted string")
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
				return token, token.Errorf("unknown escape \\%c", char)
			}
		}
		text = append(text, char)
	}
	return token, token.Errorf("unterminated formatted string")
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
