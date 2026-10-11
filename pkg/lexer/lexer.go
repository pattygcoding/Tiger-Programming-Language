package lexer

import (
	"strconv"
	"unicode"

	"tiger/pkg/format"
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

// Location renders the token's position as "source:line:column", or
// "line:column" when the token has no source name.
func (token Token) Location() string {
	if token.Source != "" {
		return format.Sprintf("%s:%d:%d", token.Source, token.Line, token.Column)
	}
	return format.Sprintf("%d:%d", token.Line, token.Column)
}

func (token Token) Errorf(template string, args ...any) error {
	return format.Errorf("%s: %s", token.Location(), format.Sprintf(template, args...))
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
		case (current == 'r' || current == 'R') && (scan.peek(1) == '"' || scan.peek(1) == '\''):
			raw, err := scan.rawString(token)
			if err != nil {
				return nil, err
			}
			token = raw
		case unicode.IsLetter(current) || current == '_':
			for unicode.IsLetter(scan.peek(0)) || unicode.IsDigit(scan.peek(0)) || scan.peek(0) == '_' {
				scan.advance()
			}
			token.Text = string(scan.source[start:scan.pos])
			token.Kind = Ident
			switch token.Text {
			case "const", "var", "function", "class", "extends", "super", "this", "public", "private", "protected", "return", "if", "elif", "else", "while", "do", "for", "cfor", "cif", "celse", "in", "true", "false", "null", "and", "or", "not", "div", "break", "continue", "switch", "case", "default", "try", "catch", "throw", "import", "as":
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
					escaped, err := scan.escape(token)
					if err != nil {
						return nil, err
					}
					char = escaped
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
				} else if (current == '<' || current == '>') && scan.peek(0) == current {
					token.Text += string(scan.advance())
				} else if current == '!' {
					return nil, token.Errorf("expected '=' after '!'; use 'not' for negation")
				}
			case '+', '-':
				if scan.peek(0) == current || scan.peek(0) == '=' {
					token.Text += string(scan.advance())
				}
			case '*':
				if scan.peek(0) == '*' {
					token.Text += string(scan.advance())
				} else if scan.peek(0) == '=' {
					token.Text += string(scan.advance())
				}
			case '%':
				if scan.peek(0) == '=' {
					token.Text += string(scan.advance())
				}
			case '&', '|':
				if scan.peek(0) == current {
					token.Text += string(scan.advance())
				}
			case '/', '(', ')', '[', ']', '{', '}', ':', ',', ';', '.', '^', '~':
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
			escaped, err := scan.escape(token)
			if err != nil {
				return token, err
			}
			char = escaped
		}
		text = append(text, char)
	}
	return token, token.Errorf("unterminated formatted string")
}

// escape reads the character after a backslash in a quoted string literal.
func (scan *scanner) escape(token Token) (rune, error) {
	if scan.pos == len(scan.source) {
		return 0, token.Errorf("unterminated string")
	}
	char := scan.advance()
	switch char {
	case 'n':
		return '\n', nil
	case 'r':
		return '\r', nil
	case 't':
		return '\t', nil
	case '\\', '"', '\'':
		return char, nil
	case 'x':
		return scan.hexEscape(2, token)
	case 'u':
		return scan.hexEscape(4, token)
	case 'U':
		return scan.hexEscape(8, token)
	}
	return 0, token.Errorf("unknown escape \\%c", char)
}

// hexEscape reads digits hexadecimal digits and returns the character they name.
func (scan *scanner) hexEscape(digits int, token Token) (rune, error) {
	value := 0
	for index := 0; index < digits; index++ {
		digit := hexValue(scan.peek(0))
		if digit < 0 {
			return 0, token.Errorf("invalid hexadecimal escape")
		}
		value = value*16 + digit
		scan.advance()
	}
	if value > unicode.MaxRune || (value >= 0xD800 && value <= 0xDFFF) {
		return 0, token.Errorf("escape U+%04X is not a valid character", value)
	}
	return rune(value), nil
}

func hexValue(char rune) int {
	switch {
	case char >= '0' && char <= '9':
		return int(char - '0')
	case char >= 'a' && char <= 'f':
		return int(char-'a') + 10
	case char >= 'A' && char <= 'F':
		return int(char-'A') + 10
	}
	return -1
}

// rawString reads r"..." or r'...', where backslashes are literal and there are
// no escape sequences.
func (scan *scanner) rawString(token Token) (Token, error) {
	scan.advance() // r or R
	quote := scan.advance()
	value := []rune{}
	for scan.pos < len(scan.source) && scan.peek(0) != quote {
		char := scan.advance()
		if char == '\n' || char == '\r' {
			return token, token.Errorf("unterminated raw string")
		}
		value = append(value, char)
	}
	if scan.pos == len(scan.source) {
		return token, token.Errorf("unterminated raw string")
	}
	scan.advance()
	token.Kind, token.Text = String, string(value)
	return token, nil
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
