//go:build !js

package evaluator

import (
	"regexp"

	"tiger/pkg/format"
)

func compileRegex(pattern string) (*regexp.Regexp, error) {
	expression, err := regexp.Compile(pattern)
	if err != nil {
		return nil, format.Errorf("regex: invalid pattern %q", pattern)
	}
	return expression, nil
}

func regexMatchString(pattern, text string) (bool, error) {
	expression, err := compileRegex(pattern)
	if err != nil {
		return false, err
	}
	return expression.MatchString(text), nil
}

func regexFindString(pattern, text string) (string, bool, error) {
	expression, err := compileRegex(pattern)
	if err != nil {
		return "", false, err
	}
	location := expression.FindStringIndex(text)
	if location == nil {
		return "", false, nil
	}
	return text[location[0]:location[1]], true, nil
}

func regexFindAllStrings(pattern, text string) ([]string, error) {
	expression, err := compileRegex(pattern)
	if err != nil {
		return nil, err
	}
	return expression.FindAllString(text, -1), nil
}

func regexGroupsString(pattern, text string) ([]string, bool, error) {
	expression, err := compileRegex(pattern)
	if err != nil {
		return nil, false, err
	}
	match := expression.FindStringSubmatch(text)
	if match == nil {
		return nil, false, nil
	}
	return match[1:], true, nil
}

func regexReplaceString(pattern, text, replacement string) (string, error) {
	expression, err := compileRegex(pattern)
	if err != nil {
		return "", err
	}
	return expression.ReplaceAllString(text, replacement), nil
}

func regexSplitString(pattern, text string) ([]string, error) {
	expression, err := compileRegex(pattern)
	if err != nil {
		return nil, err
	}
	return expression.Split(text, -1), nil
}
