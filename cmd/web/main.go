package main

import (
	"flag"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

func main() {
	address := flag.String("addr", "127.0.0.1:8080", "HTTP listen address")
	buildOnly := flag.Bool("build-only", false, "build browser artifacts without serving")
	flag.Parse()
	if err := buildWasm(); err != nil {
		log.Fatal(err)
	}
	if *buildOnly {
		fmt.Println("Built web/tiger.wasm and web/wasm_exec.js")
		return
	}
	mux := http.NewServeMux()
	mux.Handle("/examples/", http.StripPrefix("/examples/", http.FileServer(http.Dir("examples"))))
	mux.Handle("/", http.FileServer(http.Dir("web")))
	listener, err := net.Listen("tcp", *address)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Tiger playground: http://%s\n", listener.Addr())
	server := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	log.Fatal(server.Serve(listener))
}

func buildWasm() error {
	command := exec.Command("go", "build", "-o", "web/tiger.wasm", "./cmd/wasm")
	command.Env = append(os.Environ(), "GOOS=js", "GOARCH=wasm", "CGO_ENABLED=0")
	if output, err := command.CombinedOutput(); err != nil {
		return fmt.Errorf("build Wasm: %w\n%s", err, output)
	}
	root, err := exec.Command("go", "env", "GOROOT").Output()
	if err != nil {
		return err
	}
	var support []byte
	for _, directory := range []string{"lib", "misc"} {
		support, err = os.ReadFile(filepath.Join(strings.TrimSpace(string(root)), directory, "wasm", "wasm_exec.js"))
		if err == nil {
			break
		}
	}
	if err != nil {
		return fmt.Errorf("locate Go wasm_exec.js: %w", err)
	}
	return os.WriteFile("web/wasm_exec.js", support, 0644)
}
