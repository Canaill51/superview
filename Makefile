.PHONY: help build build-gui build-gui-windows test lint vet coverage coverage-html fmt fmt-fix vuln check install-tools clean version release-prepare

ARCH := $(shell go env GOARCH)

# Default target
help:
	@echo "superview - Build & Development Commands"
	@echo ""
	@echo "Build targets:"
	@echo "  build          Build GUI binary"
	@echo "  build-gui      Build GUI binary"
	@echo "  build-gui-windows Build Windows GUI .exe without console"
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
	@echo "Release targets:"
	@echo "  release-prepare  Prepare and tag release (e.g., make release-prepare VERSION=1.0.0)"
	@echo ""
	@echo "Utility targets:"
	@echo "  install-tools  Install linting and analysis tools"
	@echo "  version        Show version information"
	@echo "  clean          Remove build artifacts and coverage files"
	@echo ""

# Build targets
build: build-gui
	@echo "✅ GUI binary built successfully"

build-gui:
	@echo "Building GUI..."
	go build -o superview-gui superview-gui.go
	@echo "✅ GUI binary created: superview-gui"

build-gui-windows: export GOOS=windows
build-gui-windows:
	@echo "Building Windows GUI without console window..."
	go build -ldflags="-H=windowsgui" -o superview-gui-windows-$(ARCH).exe superview-gui.go
	@echo "✅ Windows GUI binary created: superview-gui-windows-$(ARCH).exe"

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
	rm -f superview-gui superview-gui.exe
	rm -f superview-gui-windows-*.exe
	rm -f coverage.out coverage.html
	go clean
	rm -rf dist/ build/
	@echo "✅ Cleanup complete"

# Version info
version:
	@echo "Go version: $$(go version)"
	@echo "golangci-lint version: $$(golangci-lint --version 2>/dev/null || echo 'not installed')"

# Release targets
release-prepare:
	@if [ -z "$(VERSION)" ]; then \
		echo "❌ VERSION is required: make release-prepare VERSION=1.0.0"; \
		exit 1; \
	fi
	@if ! git diff-index --quiet HEAD --; then \
		echo "❌ Working directory has uncommitted changes. Commit first."; \
		exit 1; \
	fi
	@echo "Preparing release v$(VERSION)..."
	@echo "Running full quality check..."
	@make check > /dev/null
	@echo "✅ Quality checks passed"
	@echo "Creating git tag v$(VERSION)..."
	git tag -a v$(VERSION) -m "Release v$(VERSION)" 
	@echo "✅ Tag created: v$(VERSION)"
	@echo "⚠️  Push tag to trigger release: git push origin v$(VERSION)"
