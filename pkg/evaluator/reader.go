package evaluator

import (
	"io"
	"strings"
)

// bufferedReader is a minimal replacement for bufio.Reader. It exists so the
// runtime does not link the bufio and bytes packages, which keeps the browser
// Wasm binary smaller. It supports exactly the operations the runtime uses.
type bufferedReader struct {
	reader io.Reader
	buffer []byte
	start  int
	end    int
}

func newBufferedReader(reader io.Reader) *bufferedReader {
	return &bufferedReader{reader: reader, buffer: make([]byte, 4096)}
}

// Reset discards buffered data and reads from reader instead.
func (buffer *bufferedReader) Reset(reader io.Reader) {
	buffer.reader = reader
	buffer.start, buffer.end = 0, 0
}

// Buffered reports how many bytes are waiting in the buffer.
func (buffer *bufferedReader) Buffered() int { return buffer.end - buffer.start }

func (buffer *bufferedReader) Read(data []byte) (int, error) {
	if buffer.start < buffer.end {
		count := copy(data, buffer.buffer[buffer.start:buffer.end])
		buffer.start += count
		return count, nil
	}
	buffer.start, buffer.end = 0, 0
	return buffer.reader.Read(data)
}

// ReadString reads through delim, returning the text including the delimiter.
// When the underlying reader ends first, it returns the partial text with
// io.EOF, matching bufio.Reader.ReadString.
func (buffer *bufferedReader) ReadString(delim byte) (string, error) {
	var builder strings.Builder
	for {
		if index := indexByte(buffer.buffer[buffer.start:buffer.end], delim); index >= 0 {
			builder.Write(buffer.buffer[buffer.start : buffer.start+index+1])
			buffer.start += index + 1
			return builder.String(), nil
		}
		builder.Write(buffer.buffer[buffer.start:buffer.end])
		buffer.start, buffer.end = 0, 0
		count, err := buffer.reader.Read(buffer.buffer)
		buffer.end = count
		if index := indexByte(buffer.buffer[:buffer.end], delim); index >= 0 {
			builder.Write(buffer.buffer[:index+1])
			buffer.start = index + 1
			return builder.String(), nil
		}
		if err != nil {
			builder.Write(buffer.buffer[:buffer.end])
			buffer.start, buffer.end = 0, 0
			return builder.String(), err
		}
		if count == 0 {
			return builder.String(), io.EOF
		}
	}
}

func indexByte(data []byte, target byte) int {
	for index := 0; index < len(data); index++ {
		if data[index] == target {
			return index
		}
	}
	return -1
}
