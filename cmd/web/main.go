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

const defaultAddress = "127.0.0.1:7171"

func main() {
	address := flag.String("addr", defaultAddress, "HTTP listen address; the default falls back to a free port when busy")
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
	mux.Handle("/portfolio-features/", http.StripPrefix("/portfolio-features/", http.FileServer(http.Dir("portfolio-features"))))
	mux.Handle("/", http.FileServer(http.Dir("web")))
	listener, err := net.Listen("tcp", *address)
	if err != nil && *address == defaultAddress {
		fmt.Printf("%s is busy; using a free port instead\n", defaultAddress)
		listener, err = net.Listen("tcp", "127.0.0.1:0")
	}
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Tiger playground: http://%s\n", listener.Addr())
	// Revalidate every request so rebuilt Wasm, scripts, and examples are never served stale.
	handler := http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("Cache-Control", "no-cache")
		mux.ServeHTTP(writer, request)
	})
	server := &http.Server{Handler: handler, ReadHeaderTimeout: 5 * time.Second}
	log.Fatal(server.Serve(listener))
}

func buildWasm() error {
	// -s -w drop the symbol table and DWARF data, -buildid drops the build ID,
	// and -buildvcs skips VCS stamping; the browser never uses any of it.
	command := exec.Command("go", "build", "-trimpath", "-buildvcs=false", "-ldflags=-s -w -buildid=", "-o", "web/tiger.wasm", "./cmd/wasm")
	command.Env = append(os.Environ(), "GOOS=js", "GOARCH=wasm", "CGO_ENABLED=0")
	if output, err := command.CombinedOutput(); err != nil {
		return fmt.Errorf("build Wasm: %w\n%s", err, output)
	}
	if optimizer, err := exec.LookPath("wasm-opt"); err == nil {
		// Go's Wasm output uses these post-MVP features, so wasm-opt must allow
		// them. -Oz with --converge and the strip passes gives the smallest file.
		// Optimize to a temporary file so a failure never corrupts tiger.wasm.
		optimized := "web/tiger.wasm.opt"
		command := exec.Command(optimizer, "-Oz", "--converge",
			"--strip-debug", "--strip-dwarf", "--strip-producers",
			"--enable-bulk-memory", "--enable-sign-ext", "--enable-nontrapping-float-to-int",
			"web/tiger.wasm", "-o", optimized)
		if output, err := command.CombinedOutput(); err != nil {
			_ = os.Remove(optimized)
			fmt.Printf("wasm-opt skipped: %v\n%s", err, output)
		} else if err := os.Rename(optimized, "web/tiger.wasm"); err != nil {
			return err
		} else {
			fmt.Println("Optimized web/tiger.wasm with wasm-opt -Oz")
		}
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
