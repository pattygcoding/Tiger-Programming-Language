//go:build !js

package evaluator

import (
	"math"
	"time"

	"tiger/pkg/object"
)

func timeNow() (float64, error) {
	return float64(time.Now().UnixNano()) / 1e9, nil
}

func timeNowMillis() (float64, error) {
	return float64(time.Now().UnixMilli()), nil
}

func timeSleep(seconds float64) error {
	time.Sleep(time.Duration(seconds * float64(time.Second)))
	return nil
}

func timeISO(seconds float64) (string, error) {
	return instant(seconds).UTC().Format(time.RFC3339), nil
}

func timeParts(seconds float64) (*object.Dict, error) {
	moment := instant(seconds).UTC()
	parts := &object.Dict{}
	fields := []struct {
		key   string
		value float64
	}{
		{"year", float64(moment.Year())},
		{"month", float64(moment.Month())},
		{"day", float64(moment.Day())},
		{"hour", float64(moment.Hour())},
		{"minute", float64(moment.Minute())},
		{"second", float64(moment.Second())},
		{"weekday", float64(moment.Weekday())},
		{"yearday", float64(moment.YearDay())},
	}
	for _, field := range fields {
		if err := parts.Set(object.String(field.key), object.Number(field.value)); err != nil {
			return nil, err
		}
	}
	return parts, nil
}

// instant splits a fractional Unix timestamp into whole seconds and nanoseconds.
func instant(seconds float64) time.Time {
	whole := math.Floor(seconds)
	return time.Unix(int64(whole), int64((seconds-whole)*1e9))
}
