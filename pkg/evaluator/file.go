package evaluator

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"math"
	"os"
	"path"
	"strings"

	"tiger/pkg/ast"
	"tiger/pkg/object"
)

// FileMode is the set of behaviors requested by a Python-style mode string such
// as "r", "w", "a", "r+", or "rb".
type FileMode struct {
	Read     bool
	Write    bool
	Append   bool
	Truncate bool
	Create   bool
	Binary   bool
}

// ParseFileMode accepts r, w, and a with an optional b, an optional +, and the
// two combined forms "r+b" and "rb+". Every other spelling is rejected.
func ParseFileMode(mode string) (FileMode, error) {
	flag := FileMode{}
	base := ""
	plus := false
	for index := 0; index < len(mode); index++ {
		switch mode[index] {
		case 'b', 'B':
			if flag.Binary || base == "" {
				return FileMode{}, fmt.Errorf("invalid mode %q", mode)
			}
			flag.Binary = true
		case '+':
			if plus || base == "" {
				return FileMode{}, fmt.Errorf("invalid mode %q", mode)
			}
			plus = true
		case 'r', 'R', 'w', 'W', 'a', 'A':
			if base != "" {
				return FileMode{}, fmt.Errorf("invalid mode %q", mode)
			}
			base = strings.ToLower(string(mode[index]))
		default:
			return FileMode{}, fmt.Errorf("invalid mode %q", mode)
		}
	}
	switch base {
	case "r":
		flag.Read = true
	case "w":
		flag.Write, flag.Truncate, flag.Create = true, true, true
	case "a":
		flag.Write, flag.Append, flag.Create = true, true, true
	default:
		return FileMode{}, fmt.Errorf("invalid mode %q", mode)
	}
	if plus {
		flag.Read, flag.Write = true, true
	}
	return flag, nil
}

// FileHandle is one open byte stream. Both the operating system and the
// browser's in-memory filesystem provide this interface.
type FileHandle interface {
	io.Reader
	io.Writer
	io.Seeker
	io.Closer
	Name() string
}

// FileSystem opens and removes files. The native CLI and WASI builds use the
// host filesystem; browser builds and tests use MemoryFileSystem.
type FileSystem interface {
	Open(name string, mode FileMode) (FileHandle, error)
	Remove(name string) error
	Exists(name string) (bool, error)
}

// OSFileSystem reads and writes the host operating system's files.
type OSFileSystem struct{}

func (OSFileSystem) Open(name string, mode FileMode) (FileHandle, error) {
	flags := os.O_RDONLY
	switch {
	case mode.Read && mode.Write:
		flags = os.O_RDWR
	case mode.Write:
		flags = os.O_WRONLY
	}
	if mode.Create {
		flags |= os.O_CREATE
	}
	if mode.Truncate {
		flags |= os.O_TRUNC
	}
	if mode.Append {
		flags |= os.O_APPEND
	}
	handle, err := os.OpenFile(name, flags, 0644)
	if err != nil {
		return nil, err
	}
	return handle, nil
}

func (OSFileSystem) Remove(name string) error { return os.Remove(name) }

func (OSFileSystem) Exists(name string) (bool, error) {
	info, err := os.Stat(name)
	if err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return false, nil
		}
		return false, err
	}
	return !info.IsDir(), nil
}

// MemoryFileSystem keeps files in memory. Browser builds use it because a
// GOOS=js program has no host filesystem; tests use it to stay hermetic.
type MemoryFileSystem struct {
	entries map[string]*memoryFile
}

// NewMemoryFileSystem returns an empty virtual filesystem.
func NewMemoryFileSystem() *MemoryFileSystem {
	return &MemoryFileSystem{entries: map[string]*memoryFile{}}
}

type memoryFile struct {
	name   string
	data   []byte
	offset int64
	append bool
}

func (filesystem *MemoryFileSystem) Open(name string, mode FileMode) (FileHandle, error) {
	key, err := memoryKey(name)
	if err != nil {
		return nil, err
	}
	entry, exists := filesystem.entries[key]
	if !exists && !mode.Create {
		return nil, &fs.PathError{Op: "open", Path: name, Err: fs.ErrNotExist}
	}
	if !exists {
		entry = &memoryFile{name: key}
		filesystem.entries[key] = entry
	}
	if mode.Truncate {
		entry.data = nil
	}
	entry.append = mode.Append
	if mode.Append {
		entry.offset = int64(len(entry.data))
	} else {
		entry.offset = 0
	}
	return entry, nil
}

func (filesystem *MemoryFileSystem) Remove(name string) error {
	key, err := memoryKey(name)
	if err != nil {
		return err
	}
	if _, exists := filesystem.entries[key]; !exists {
		return &fs.PathError{Op: "remove", Path: name, Err: fs.ErrNotExist}
	}
	delete(filesystem.entries, key)
	return nil
}

func (filesystem *MemoryFileSystem) Exists(name string) (bool, error) {
	key, err := memoryKey(name)
	if err != nil {
		return false, err
	}
	_, exists := filesystem.entries[key]
	return exists, nil
}

func memoryKey(name string) (string, error) {
	if strings.TrimSpace(name) == "" {
		return "", fmt.Errorf("path must not be empty")
	}
	key := path.Clean(strings.ReplaceAll(name, "\\", "/"))
	if key == "." || key == "/" {
		return "", fmt.Errorf("invalid path %q", name)
	}
	return strings.TrimPrefix(key, "/"), nil
}

func (file *memoryFile) Read(buffer []byte) (int, error) {
	if file.offset >= int64(len(file.data)) {
		return 0, io.EOF
	}
	count := copy(buffer, file.data[file.offset:])
	file.offset += int64(count)
	return count, nil
}

func (file *memoryFile) Write(buffer []byte) (int, error) {
	if file.append || file.offset > int64(len(file.data)) {
		file.offset = int64(len(file.data))
	}
	end := file.offset + int64(len(buffer))
	if end > int64(len(file.data)) {
		grown := make([]byte, end)
		copy(grown, file.data)
		file.data = grown
	}
	copy(file.data[file.offset:], buffer)
	file.offset = end
	return len(buffer), nil
}

func (file *memoryFile) Seek(offset int64, whence int) (int64, error) {
	var base int64
	switch whence {
	case io.SeekStart:
		base = 0
	case io.SeekCurrent:
		base = file.offset
	case io.SeekEnd:
		base = int64(len(file.data))
	default:
		return 0, fmt.Errorf("invalid seek origin %d", whence)
	}
	position := base + offset
	if position < 0 {
		return 0, fmt.Errorf("negative seek position %d", position)
	}
	file.offset = position
	return position, nil
}

func (file *memoryFile) Close() error { return nil }

func (file *memoryFile) Name() string { return file.name }

var errFileClosed = errors.New("I/O operation on closed file")

// openFile is the runtime behavior behind an *object.File value. Reads go
// through a persistent buffered reader so repeated readline calls work; any
// write or seek discards the buffer and repositions the underlying handle.
type openFile struct {
	name   string
	mode   FileMode
	handle FileHandle
	reader *bufio.Reader
	closed bool
}

func (eval *Evaluator) files() FileSystem {
	if eval.FileSystem != nil {
		return eval.FileSystem
	}
	return OSFileSystem{}
}

func (eval *Evaluator) openHandle(name, mode string) (*openFile, error) {
	flag, err := ParseFileMode(mode)
	if err != nil {
		return nil, err
	}
	handle, err := eval.files().Open(name, flag)
	if err != nil {
		return nil, err
	}
	file := &openFile{name: name, mode: flag, handle: handle}
	if flag.Read {
		file.reader = bufio.NewReader(handle)
	}
	return file, nil
}

func (eval *Evaluator) openFileValue(name, mode string) (object.Value, error) {
	file, err := eval.openHandle(name, mode)
	if err != nil {
		return nil, err
	}
	return &object.File{Name: name, Mode: mode, Handle: file}, nil
}

func (file *openFile) readText(size int64) (string, error) {
	if file.closed {
		return "", errFileClosed
	}
	if !file.mode.Read {
		return "", fmt.Errorf("file %q is not open for reading", file.name)
	}
	var data []byte
	var err error
	if size < 0 {
		data, err = io.ReadAll(file.reader)
	} else {
		data, err = io.ReadAll(io.LimitReader(file.reader, size))
	}
	if err != nil {
		return "", err
	}
	return string(data), nil
}

func (file *openFile) readLine() (string, error) {
	if file.closed {
		return "", errFileClosed
	}
	if !file.mode.Read {
		return "", fmt.Errorf("file %q is not open for reading", file.name)
	}
	text, err := file.reader.ReadString('\n')
	if err != nil && err != io.EOF {
		return "", err
	}
	return text, nil
}

func (file *openFile) readLines() (object.Value, error) {
	list := &object.List{}
	for {
		line, err := file.readLine()
		if err != nil {
			return nil, err
		}
		if line == "" {
			return list, nil
		}
		list.Elements = append(list.Elements, object.String(line))
	}
}

func (file *openFile) writeText(text string) (int, error) {
	if file.closed {
		return 0, errFileClosed
	}
	if !file.mode.Write {
		return 0, fmt.Errorf("file %q is not open for writing", file.name)
	}
	count, err := io.WriteString(file.handle, text)
	if file.reader != nil {
		file.reader.Reset(file.handle)
	}
	return count, err
}

func (file *openFile) seek(offset int64, whence int) (int64, error) {
	if file.closed {
		return 0, errFileClosed
	}
	position, err := file.handle.Seek(offset, whence)
	if err != nil {
		return 0, err
	}
	if file.reader != nil {
		file.reader.Reset(file.handle)
	}
	return position, nil
}

func (file *openFile) tell() (int64, error) {
	if file.closed {
		return 0, errFileClosed
	}
	position, err := file.handle.Seek(0, io.SeekCurrent)
	if err != nil {
		return 0, err
	}
	if file.reader != nil {
		position -= int64(file.reader.Buffered())
	}
	return position, nil
}

func (file *openFile) flush() error {
	if file.closed {
		return errFileClosed
	}
	if syncer, ok := file.handle.(interface{ Sync() error }); ok {
		return syncer.Sync()
	}
	return nil
}

func (file *openFile) close() error {
	if file.closed {
		return nil
	}
	file.closed = true
	return file.handle.Close()
}

var fileMethods = map[string]bool{
	"read": true, "readline": true, "readlines": true, "write": true,
	"writelines": true, "flush": true, "close": true, "seek": true, "tell": true,
}

func fileProperty(node *ast.Property, file *object.File) object.Value {
	opened, ok := file.Handle.(*openFile)
	if !ok {
		fail(node, "file has no property %q", node.Name)
	}
	switch node.Name {
	case "name":
		return object.String(file.Name)
	case "mode":
		return object.String(file.Mode)
	case "closed":
		return object.Bool(opened.closed)
	}
	if !fileMethods[node.Name] {
		fail(node, "file has no property %q", node.Name)
	}
	return fileMethod(node.Name, opened)
}

func fileMethod(name string, file *openFile) object.Value {
	return &object.Builtin{Name: name, Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, fmt.Errorf("file %s does not accept keyword arguments", name)
		}
		switch name {
		case "read":
			if len(arguments) > 1 {
				return nil, fmt.Errorf("read expects 0 or 1 arguments, got %d", len(arguments))
			}
			size := int64(-1)
			if len(arguments) == 1 {
				value, err := fileInteger(arguments[0], "read size")
				if err != nil {
					return nil, err
				}
				size = value
			}
			text, err := file.readText(size)
			if err != nil {
				return nil, err
			}
			return object.String(text), nil
		case "readline":
			if len(arguments) != 0 {
				return nil, fmt.Errorf("readline expects 0 arguments, got %d", len(arguments))
			}
			text, err := file.readLine()
			if err != nil {
				return nil, err
			}
			return object.String(text), nil
		case "readlines":
			if len(arguments) != 0 {
				return nil, fmt.Errorf("readlines expects 0 arguments, got %d", len(arguments))
			}
			return file.readLines()
		case "write":
			if len(arguments) != 1 {
				return nil, fmt.Errorf("write expects 1 argument, got %d", len(arguments))
			}
			text, ok := arguments[0].(object.String)
			if !ok {
				return nil, fmt.Errorf("write expects a string, got %s", arguments[0].Type())
			}
			count, err := file.writeText(string(text))
			if err != nil {
				return nil, err
			}
			return object.Number(count), nil
		case "writelines":
			if len(arguments) != 1 {
				return nil, fmt.Errorf("writelines expects 1 argument, got %d", len(arguments))
			}
			list, ok := arguments[0].(*object.List)
			if !ok {
				return nil, fmt.Errorf("writelines expects a list, got %s", arguments[0].Type())
			}
			for _, element := range list.Elements {
				text, ok := element.(object.String)
				if !ok {
					return nil, fmt.Errorf("writelines expects a list of strings, got %s", element.Type())
				}
				if _, err := file.writeText(string(text)); err != nil {
					return nil, err
				}
			}
			return object.Null{}, nil
		case "flush":
			if len(arguments) != 0 {
				return nil, fmt.Errorf("flush expects 0 arguments, got %d", len(arguments))
			}
			if err := file.flush(); err != nil {
				return nil, err
			}
			return object.Null{}, nil
		case "close":
			if len(arguments) != 0 {
				return nil, fmt.Errorf("close expects 0 arguments, got %d", len(arguments))
			}
			if err := file.close(); err != nil {
				return nil, err
			}
			return object.Null{}, nil
		case "seek":
			if len(arguments) < 1 || len(arguments) > 2 {
				return nil, fmt.Errorf("seek expects 1 or 2 arguments, got %d", len(arguments))
			}
			offset, err := fileInteger(arguments[0], "seek offset")
			if err != nil {
				return nil, err
			}
			whence := int64(io.SeekStart)
			if len(arguments) == 2 {
				whence, err = fileInteger(arguments[1], "seek origin")
				if err != nil {
					return nil, err
				}
				if whence < 0 || whence > 2 {
					return nil, fmt.Errorf("seek origin must be 0, 1, or 2, got %d", whence)
				}
			}
			position, err := file.seek(offset, int(whence))
			if err != nil {
				return nil, err
			}
			return object.Number(position), nil
		case "tell":
			if len(arguments) != 0 {
				return nil, fmt.Errorf("tell expects 0 arguments, got %d", len(arguments))
			}
			position, err := file.tell()
			if err != nil {
				return nil, err
			}
			return object.Number(position), nil
		}
		return nil, fmt.Errorf("file has no property %q", name)
	}}
}

func fileInteger(value object.Value, label string) (int64, error) {
	number, ok := value.(object.Number)
	if !ok || math.IsNaN(float64(number)) || math.IsInf(float64(number), 0) || math.Trunc(float64(number)) != float64(number) || math.Abs(float64(number)) > 9007199254740991 {
		return 0, fmt.Errorf("%s must be a safe integer", label)
	}
	return int64(number), nil
}

func (eval *Evaluator) fileBuiltins() map[string]func([]object.Value, map[string]object.Value) (object.Value, error) {
	return map[string]func([]object.Value, map[string]object.Value) (object.Value, error){
		"open": func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
			if len(keywords) != 0 {
				return nil, fmt.Errorf("open does not accept keyword arguments")
			}
			if len(arguments) < 1 || len(arguments) > 2 {
				return nil, fmt.Errorf("open expects 1 or 2 arguments, got %d", len(arguments))
			}
			name, ok := arguments[0].(object.String)
			if !ok {
				return nil, fmt.Errorf("open expects a path string, got %s", arguments[0].Type())
			}
			mode := "r"
			if len(arguments) == 2 {
				text, ok := arguments[1].(object.String)
				if !ok {
					return nil, fmt.Errorf("open mode must be a string, got %s", arguments[1].Type())
				}
				mode = string(text)
			}
			return eval.openFileValue(string(name), mode)
		},
		"read_file": func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
			name, err := filePathArgument("read_file", arguments, keywords)
			if err != nil {
				return nil, err
			}
			opened, err := eval.openHandle(name, "r")
			if err != nil {
				return nil, err
			}
			defer opened.close()
			text, err := opened.readText(-1)
			if err != nil {
				return nil, err
			}
			return object.String(text), nil
		},
		"write_file": func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
			name, text, err := fileTextArguments("write_file", arguments, keywords)
			if err != nil {
				return nil, err
			}
			return eval.writeHandle(name, text, "w")
		},
		"append_file": func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
			name, text, err := fileTextArguments("append_file", arguments, keywords)
			if err != nil {
				return nil, err
			}
			return eval.writeHandle(name, text, "a")
		},
		"file_exists": func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
			name, err := filePathArgument("file_exists", arguments, keywords)
			if err != nil {
				return nil, err
			}
			exists, err := eval.files().Exists(name)
			if err != nil {
				return nil, err
			}
			return object.Bool(exists), nil
		},
		"remove_file": func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
			name, err := filePathArgument("remove_file", arguments, keywords)
			if err != nil {
				return nil, err
			}
			if err := eval.files().Remove(name); err != nil {
				return nil, err
			}
			return object.Null{}, nil
		},
	}
}

func (eval *Evaluator) writeHandle(name, text, mode string) (object.Value, error) {
	opened, err := eval.openHandle(name, mode)
	if err != nil {
		return nil, err
	}
	count, writeErr := opened.writeText(text)
	closeErr := opened.close()
	if writeErr != nil {
		return nil, writeErr
	}
	if closeErr != nil {
		return nil, closeErr
	}
	return object.Number(count), nil
}

func filePathArgument(name string, arguments []object.Value, keywords map[string]object.Value) (string, error) {
	if len(keywords) != 0 {
		return "", fmt.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 1 {
		return "", fmt.Errorf("%s expects 1 argument, got %d", name, len(arguments))
	}
	path, ok := arguments[0].(object.String)
	if !ok {
		return "", fmt.Errorf("%s expects a path string, got %s", name, arguments[0].Type())
	}
	return string(path), nil
}

func fileTextArguments(name string, arguments []object.Value, keywords map[string]object.Value) (string, string, error) {
	if len(keywords) != 0 {
		return "", "", fmt.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 2 {
		return "", "", fmt.Errorf("%s expects 2 arguments, got %d", name, len(arguments))
	}
	path, ok := arguments[0].(object.String)
	if !ok {
		return "", "", fmt.Errorf("%s expects a path string, got %s", name, arguments[0].Type())
	}
	text, ok := arguments[1].(object.String)
	if !ok {
		return "", "", fmt.Errorf("%s expects a string value, got %s", name, arguments[1].Type())
	}
	return string(path), string(text), nil
}