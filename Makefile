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
	@echo "  test           Run all tests (needs ffmpeg on PATH)"
	@echo "  coverage       Run tests with coverage report (needs ffmpeg)"
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
	@echo "  clean          Remove build, coverage and packaging leftovers"
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
#
# SUPERVIEW_REQUIRE_FFMPEG=1 is not a convenience, it is the whole point of
# running the suite: without it every test that shells out to ffmpeg calls
# t.Skip instead of failing, and a skip is invisible. That is the four
# integration tests and the remap equivalence test -- the whole of what checks
# a real conversion end to end. "make test" was green on a machine with no
# ffmpeg at all, having encoded nothing. CI sets it (test.yml), AGENTS.md asks
# for it, and this is the command a contributor actually types.
#
# -race for the same reason: the encode runs in a goroutine and reports to the
# UI through another, so the suite is only meaningful under the detector.
# -count=1 because a cached green result proves nothing about the tree at hand.
TEST_FLAGS := -race -count=1
test coverage: export SUPERVIEW_REQUIRE_FFMPEG := 1

test:
	@echo "Running tests..."
	go test $(TEST_FLAGS) -v ./...
	@echo "✅ Tests passed"

coverage:
	@echo "Running tests with coverage analysis..."
	go test $(TEST_FLAGS) ./... -coverprofile=coverage.out -covermode=atomic
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

# Removes what this Makefile and "fyne package" produce, and nothing else.
#
# The packaging leftovers are not cosmetic. "fyne package" writes into the
# project root while it works -- the metadata file, the Windows resource
# object, an intermediate binary named after the module, a staging directory --
# and cmd/go reads "git status --porcelain" to decide vcs.modified. Every
# release up to v0.2.3 therefore announced itself as ", modified" (R-06).
# .gitignore learned these names one release at a time; keep the two lists
# together, because an ignored file is hidden, not gone.
#
# "dist/" used to be on the last rm line and is produced by nothing in this
# repository -- not by any target here, not by the release workflow, and it is
# not even in .gitignore.
clean:
	@echo "Cleaning up..."
	rm -f superview-gui superview-gui.exe
	rm -f superview-gui-windows-*.exe
	rm -f superview-gui-linux-*
	rm -f coverage.out coverage.html
	@# Left behind by "fyne package"; mirrors .gitignore.
	rm -f superview superview.exe fyne_metadata_init.go
	rm -f *.syso
	rm -f superview-gui-*.tar.xz superview-gui-*.zip
	rm -rf tmp-pkg/
	go clean
	rm -rf build/
	@echo "✅ Cleanup complete"

# Version info
version:
	@echo "Go version: $$(go version)"
	@echo "golangci-lint version: $$(golangci-lint --version 2>/dev/null || echo 'not installed')"
