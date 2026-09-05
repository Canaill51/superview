.PHONY: help build build-gui build-gui-windows build-gui-linux test lint vet coverage coverage-html fmt fmt-fix vuln check install-tools clean version

ARCH := $(shell go env GOARCH)

# Keep in sync with .github/workflows/lint.yml. Pinned rather than @latest so a
# release of one of these tools cannot fail a build that contains no change of
# ours.
GOLANGCI_LINT_VERSION := v2.13.2
GOVULNCHECK_VERSION := v1.7.0

# Default target
help:
	@echo "superview - Build & Development Commands"
	@echo ""
	@echo "Build targets:"
	@echo "  build          Build GUI binary"
	@echo "  build-gui      Build GUI binary"
	@echo "  build-gui-windows Build Windows GUI .exe without console"
	@echo "  build-gui-linux   Build Linux GUI binary"
	@echo ""
	@echo "Test & Quality targets:"
	@echo "  test           Run all tests"
	@echo "  coverage       Run tests with coverage report"
	@echo "  coverage-html  Generate HTML coverage report"
	@echo "  lint           Run golangci-lint"
	@echo "  vet            Run go vet"
	@echo "  fmt            Check code formatting"
	@echo "  fmt-fix        Auto-fix code formatting"
	@echo "  vuln           Run govulncheck for vulnerabilities"
	@echo "  check          Run all quality checks"
	@echo ""
	@echo "Utility targets:"
	@echo "  install-tools  Install linting and analysis tools"
	@echo "  version        Show version information"
	@echo "  clean          Remove build artifacts and coverage files"
	@echo ""
	@echo "Releases are made from the Actions tab, not from here -- see RELEASING.md."
	@echo ""

# Build targets
build: build-gui
	@echo "✅ GUI binary built successfully"

build-gui:
	@echo "Building GUI..."
	go build -o superview-gui .
	@echo "✅ GUI binary created: superview-gui"

# Windows-native only. Fyne draws through cgo, and setting GOOS alone gives no
# Windows C toolchain, so running this from Linux fails at the link step. The
# release workflow builds each platform on its own runner for this reason.
build-gui-windows: export GOOS=windows
build-gui-windows:
	@echo "Building Windows GUI without console window (run this on Windows)..."
	go build -ldflags="-H=windowsgui" -o superview-gui-windows-$(ARCH).exe .
	@echo "✅ Windows GUI binary created: superview-gui-windows-$(ARCH).exe"

build-gui-linux: export GOOS=linux
build-gui-linux:
	@echo "Building Linux GUI..."
	go build -o superview-gui-linux-$(ARCH) .
	@echo "✅ Linux GUI binary created: superview-gui-linux-$(ARCH)"

# Test targets
test:
	@echo "Running tests..."
	go test -v ./...
	@echo "✅ Tests passed"

coverage:
	@echo "Running tests with coverage analysis..."
	go test ./... -coverprofile=coverage.out -covermode=atomic
	@echo ""
	@echo "Coverage summary:"
	@go tool cover -func=coverage.out | grep total
	@echo ""
	@echo "Coverage by function:"
	@go tool cover -func=coverage.out | tail -20

coverage-html: coverage
	@echo "Generating HTML coverage report..."
	go tool cover -html=coverage.out -o coverage.html
	@echo "✅ Coverage report generated: coverage.html"

# Quality targets
lint:
	@echo "Running golangci-lint..."
	golangci-lint run ./... --timeout=5m

vet:
	@echo "Running go vet..."
	go vet ./...
	@echo "✅ No issues found"

fmt:
	@echo "Checking code formatting..."
	@dfmt=$$(gofmt -l .); \
	if [ -n "$$dfmt" ]; then \
		echo "Formatting issues found:"; \
		echo "$$dfmt"; \
		exit 1; \
	fi
	@echo "✅ Code formatting is correct"

fmt-fix:
	@echo "Auto-fixing code formatting..."
	gofmt -w .
	@echo "✅ Code formatting fixed"

vuln:
	@echo "Checking for vulnerabilities..."
	go install golang.org/x/vuln/cmd/govulncheck@$(GOVULNCHECK_VERSION)
	$$(go env GOPATH)/bin/govulncheck ./...
	@echo "✅ No vulnerabilities detected in code"

# Comprehensive quality check
check: fmt vet lint coverage vuln
	@echo ""
	@echo "✅ All quality checks passed!"

# Utility targets
install-tools:
	@echo "Installing development tools..."
	go install github.com/golangci/golangci-lint/v2/cmd/golangci-lint@$(GOLANGCI_LINT_VERSION)
	go install golang.org/x/vuln/cmd/govulncheck@$(GOVULNCHECK_VERSION)
	@echo "✅ Development tools installed"

clean:
	@echo "Cleaning up..."
	rm -f superview-gui superview-gui.exe
	rm -f superview-gui-windows-*.exe
	rm -f superview-gui-linux-*
	rm -f coverage.out coverage.html
	go clean
	rm -rf dist/ build/
	@echo "✅ Cleanup complete"

# Version info
version:
	@echo "Go version: $$(go version)"
	@echo "golangci-lint version: $$(golangci-lint --version 2>/dev/null || echo 'not installed')"
