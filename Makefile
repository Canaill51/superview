.PHONY: help build build-cli build-gui test lint vet coverage clean coverage-html fmt install-tools

# Default target
help:
	@echo "superview - Build & Development Commands"
	@echo ""
	@echo "Build targets:"
	@echo "  build          Build all binaries (CLI and GUI)"
	@echo "  build-cli      Build CLI binary"
	@echo "  build-gui      Build GUI binary"
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
	@echo ""
	@echo "Utility targets:"
	@echo "  install-tools  Install linting and analysis tools"
	@echo "  clean          Remove build artifacts and coverage files"
	@echo ""

# Build targets
build: build-cli build-gui
	@echo "✅ All binaries built successfully"

build-cli:
	@echo "Building CLI..."
	go build -o superview-cli superview-cli.go
	@echo "✅ CLI binary created: superview-cli"

build-gui:
	@echo "Building GUI..."
	go build -o superview-gui superview-gui.go
	@echo "✅ GUI binary created: superview-gui"

# Test targets
test:
	@echo "Running tests..."
	go test -v ./common
	@echo "✅ Tests passed"

coverage:
	@echo "Running tests with coverage analysis..."
	go test ./common -coverprofile=coverage.out -covermode=atomic
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
	golangci-lint run --timeout=5m

vet:
	@echo "Running go vet on common package..."
	go vet ./common
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
	go install golang.org/x/vuln/cmd/govulncheck@latest
	govulncheck ./...
	@echo "✅ No vulnerabilities detected in code"

# Comprehensive quality check
check: fmt vet lint coverage vuln
	@echo ""
	@echo "✅ All quality checks passed!"

# Utility targets
install-tools:
	@echo "Installing development tools..."
	go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest
	go install golang.org/x/vuln/cmd/govulncheck@latest
	go install honnef.co/go/tools/cmd/staticcheck@latest
	@echo "✅ Development tools installed"

clean:
	@echo "Cleaning up..."
	rm -f superview-cli superview-gui
	rm -f coverage.out coverage.html
	go clean
	@echo "✅ Cleanup complete"

# Version info
version:
	@echo "Go version: $$(go version)"
	@echo "golangci-lint version: $$(golangci-lint --version 2>/dev/null || echo 'not installed')"
