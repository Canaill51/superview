# Étape 12: Distribution & Versions Officielles — Documentation

## Overview

Étape 12 met en place un système d'automatisation complet pour les releases officielles avec cross-compilation, distribution multiplateforme et processus de versioning standardisé.

**Status:** ✅ Complete
**Commits Affected:** New configuration files for GoReleaser and GitHub Actions workflows

---

## Architecture

### Release Workflow

```
Developer Tags Release
    ↓
GitHub Actions Triggered (push tag event)
    ↓
Run Tests & Coverage Validation
    ↓
GoReleaser Builds All Platforms (CLI + GUI)
    ↓
Generate Checksums & Package Archives
    ↓
Create GitHub Release (Draft Mode)
    ↓
Manual Review & Publish
    ↓
Users Download Artifacts
```

---

## Components

### 1. GoReleaser Configuration (`.goreleaser.yml`)

**Purpose:** Central configuration for cross-platform binary building and distribution

**Build Targets:**

| Binary | Windows | Linux | macOS | Notes |
|--------|---------|-------|-------|-------|
| **CLI** | ✅ amd64, 386 | ✅ amd64, 386, arm64 | ✅ amd64, arm64 | Lightweight, portable |
| **GUI** (Linux) | — | ✅ amd64, 386 | — | Requires X11/Wayland |
| **GUI** (macOS) | — | — | ✅ amd64, arm64 | Native Cocoa |
| **GUI** (Windows) | ✅ amd64, 386 | — | — | Native Win32 |

**Key Features:**
- Automatic compiler flags: `-s -w` (strip, no dwarf)
- Version injection via ldflags
- OS-specific exclusions (e.g., darwin no 386)
- Per-platform archive strategies:
  - Linux: `tar.gz`
  - Windows: `zip`
  - macOS: Project format-specific

**Archives Created:**

```
superview-1.0.0-linux-x86_64.tar.gz
superview-1.0.0-darwin-x86_64.zip
superview-1.0.0-windows-x86_64.zip
superview-gui-1.0.0-linux-x86_64.tar.gz
superview-gui-1.0.0-macos-amd64.zip
superview-gui-1.0.0-windows-x86_64.zip
checksums.txt  (SHA256 hashes)
```

**Code Signing (Optional):**
- macOS: Requires `APPLE_SIGNING_IDENTITY` environment variable
- Windows: Requires signing certificate (optional)
- Linux: No signing needed

---

### 2. GitHub Actions Release Workflow (`.github/workflows/release.yml`)

**Purpose:** Automate the entire release process from tag to published release

**Jobs:**

#### a) Test Job
- Runs before any build
- **Coverage Gate:** Enforces 30% minimum
- Fails fast if tests don't pass
- Prevents broken releases

#### b) GoReleaser Job
- Runs on Ubuntu (ideal for cross-compilation)
- Uses GoReleaser action (official)
- Creates draft releases automatically
- Generates checksums.txt
- **Draft Mode:** Manual review before publish (safety feature)

#### c) macOS-Specific Build
- Builds universal binaries (Intel + ARM)
- Uses `lipo` to combine architectures
- Optional code signing with Apple certs
- Uploads to artifacts

#### d) Windows-Specific Build
- Builds both amd64 and 386
- Creates EXE files with version info
- Cross-compiles from Linux
- Uploads to artifacts

#### e) Linux-Specific Build
- Builds amd64 and 386
- Creates tar.gz archives
- Runs on Ubuntu native
- Uploads to artifacts

#### f) Release Publication
- Collects all artifacts
- Aggregates into GitHub Release
- Generates release summary
- Displays download instructions

#### g) Notification
- Reports success/failure
- Advises manual publish step

**Key Security Features:**
- ✅ Tests run before ANY build
- ✅ Coverage gate enforced
- ✅ Draft releases (manual review)
- ✅ Checksums for integrity verification
- ✅ Atomic tag-to-release process

---

### 3. Makefile Release Commands

**Local Release Preparation:**

```bash
# 1. Prepare and tag release
make release-prepare VERSION=1.0.0

# 2. Dry-run full release (locally)
make release-dry-run

# 3. Push tag to trigger CI
git push origin v1.0.0
```

**Commands Breakdown:**

**`release-prepare`:**
- Checks for uncommitted changes
- Runs full quality check (`make check`)
- Creates annotated git tag
- Prevents accidental release of broken code

**`release-dry-run`:**
- Runs goreleaser in snapshot mode
- Builds all platforms locally
- Creates `./dist/` directory with all artifacts
- Allows testing before pushing tag
- Requires: goreleaser installed

**`install-goreleaser`:**
- Downloads goreleaser binary
- Enables local `make release-dry-run`

---

## Release Process

### Step-by-Step for Maintainers

#### 1. Prepare Changes (Local Development)

```bash
# Make and test changes
git checkout -b feature/new-encoder
# ... code changes ...
make check          # Full quality gates
git add .
git commit -m "feat: add new encoder support"
```

#### 2. Create Release Candidate

```bash
# Test full release build locally
make install-goreleaser
make release-dry-run

# Inspect ./dist/ directory
ls -lh dist/
# Should see:
#   superview-1.0.0-linux-*.tar.gz
#   superview-1.0.0-darwin-*.zip
#   superview-1.0.0-windows-*.exe
#   checksums.txt
```

#### 3. Tag Release

```bash
# Verify everything is committed
git status

# Create version tag
make release-prepare VERSION=1.0.0

# Verify tag created
git tag -l | grep v1.0.0
```

#### 4. Push to GitHub

```bash
# GitHub Actions workflow triggered automatically
git push origin v1.0.0

# Watch progress
# - Open GitHub → Actions tab
# - View "Release" workflow
# - Monitor test, build, and artifact generation
```

#### 5. Review Draft Release

- Navigate to GitHub Releases
- Review draft release (created by workflow)
- Check artifacts are complete
- Verify checksums

#### 6. Publish Release

- Click "Publish" button on draft release
- GitHub sends notifications to watchers
- Release becomes official

#### 7. Announce

```bash
# Create release notes
git log v{old-version}..v{new-version} --oneline

# Post to:
# - GitHub Discussions
# - Project website
# - Social media
```

---

## Platform-Specific Instructions

### Linux

**CLI Deployment:**
```bash
# Download
wget https://github.com/Niek/superview/releases/download/v1.0.0/superview-1.0.0-linux-x86_64.tar.gz

# Extract
tar xzf superview-1.0.0-linux-x86_64.tar.gz

# Verify checksum
sha256sum -c checksums.txt

# Install
sudo mv superview-cli /usr/local/bin/
```

**GUI Deployment:**
```bash
# AppImage format
chmod +x superview-gui-1.0.0-linux-x86_64.AppImage
./superview-gui-1.0.0-linux-x86_64.AppImage
```

### macOS

**CLI Installation:**
```bash
# Intel (amd64)
curl -L -o superview-cli-macos-amd64 \
  https://github.com/Niek/superview/releases/download/v1.0.0/superview-1.0.0-darwin-amd64

# ARM (M1/M2/M3)
curl -L -o superview-cli-macos-arm64 \
  https://github.com/Niek/superview/releases/download/v1.0.0/superview-1.0.0-darwin-arm64

# Make executable
chmod +x superview-cli-macos-*

# Verify signature (if signed)
codesign -v superview-cli-macos-arm64
```

**Homebrew (Future):**
```bash
# Once formula is approved:
brew install superview
brew install superview-gui
```

### Windows

**CLI Installation:**
```powershell
# Download from release page
# Right-click → Properties → General → Unblock (if SmartScreen blocked)

# Add to PATH or run from Downloads:
.\superview-cli.exe --help
```

**Installer (Future):**
```powershell
# MSI installer (generated by goreleaser)
Start-Process -FilePath "superview-1.0.0-windows-setup.msi"
```

---

## Versioning Scheme

**Format:** `v{MAJOR}.{MINOR}.{PATCH}`

**Examples:**
- `v1.0.0` — Initial stable release
- `v1.0.1` — Patch (bugfix)
- `v1.1.0` — Minor (feature)
- `v2.0.0` — Major (breaking change)

**Pre-releases:**
- `v1.0.0-alpha` → `v1.0.0-beta` → `v1.0.0-rc1` → `v1.0.0`
- Auto-detected as pre-release by GitHub Actions
- Not highlighted as "Latest Release"

---

## Artifact Integrity

### Checksums

Each release includes `checksums.txt`:

```
abc123def456... superview-1.0.0-linux-x86_64.tar.gz
def456abc123... superview-1.0.0-darwin-amd64.zip
...
```

**Verification:**

```bash
# Linux/macOS
sha256sum -c checksums.txt

# Windows
certUtil -hashfile superview-1.0.0-windows-amd64.exe SHA256
```

### Code Signatures

**macOS (Optional):**
- Binaries signed with Apple Developer certificate
- Enables distribution outside App Store
- Users: `codesign -v binary_name` to verify

**Windows (Optional):**
- EXE signed with CodeSign certificate
- Prevents SmartScreen warnings
- Users: `signtool verify /pa binary_name.exe`

---

## CI/CD Integration

### GitHub Actions Events

Releases trigger on:
```yaml
push:
  tags:
    - 'v*'           # semver: v1.0.0
    - 'release-*'    # override: release-custom
```

### Status Checks

Release workflow requires passing:
- ✅ test (coverage ≥ 30%)
- ✅ build-and-release (goreleaser)
- ✅ build-macos (universal binary)
- ✅ build-windows (multi-arch)
- ✅ build-linux (multi-arch)

---

## Files Modified/Created

| File | Purpose | Lines |
|------|---------|-------|
| `.goreleaser.yml` | Cross-platform build config | 250 |
| `.github/workflows/release.yml` | Release automation | 350 |
| `Makefile` | Local release commands | 60 additions |

**Total Addition:** ~660 lines

---

## Troubleshooting

### "Release appears as Draft"

**Why:** Safety feature to prevent accidental public releases

**Solution:** Use GitHub UI to publish draft → click "Publish release"

### "Coverage gate failed"

**Why:** Code coverage below 30%

**Solution:** Add more tests before releasing:
```bash
make coverage
# Then add tests to increase coverage
```

### "Checksum mismatch on download"

**Why:** Network corruption or file incomplete

**Solution:** Re-download and verify:
```bash
sha256sum -c checksums.txt superview-1.0.0-linux-x86_64.tar.gz
```

### "Cross-compilation fails"

**Why:** Missing dependencies or Go tools

**Solution:** Install goreleaser and dependencies:
```bash
make install-goreleaser
go mod download
```

---

## Future Enhancements

### Phase 2 (Étape 12+):

1. **Package Managers:**
   - Homebrew formula (macOS)
   - Chocolatey package (Windows)
   - Snap package (Linux)
   - AUR package (Arch Linux)

2. **Automated Testing:**
   - Installation test for each platform
   - Release candidate testing
   - Smoke tests before publish

3. **Release Notes:**
   - Auto-generation from commit messages
   - Breaking change detection
   - Migration guides for major versions

4. **Distribution:**
   - Docker images for containerized use
   - OCI artifacts for container registries
   - Kubernetes Helm charts

---

## Validation Checklist

- ✅ `.goreleaser.yml` syntactically valid YAML
- ✅ All supported platforms in build matrix
- ✅ GitHub Actions workflow tested locally
- ✅ Checksums calculated for integrity
- ✅ Draft releases (safe default)
- ✅ Cross-platform binaries work standalone
- ✅ Makefile commands available locally
- ✅ Coverage gate enforced

**Status:** ✅ Étape 12 Complete — Release system ready to use

---

## Usage Quick Reference

```bash
# For Users (Installation)
# =========================
# Download from: GitHub Releases page
# Extract platform-specific binary
# Run: ./superview-cli --help

# For Developers (Creating Release)
# ==================================
make install-goreleaser         # Once setup
make release-dry-run            # Test locally
make release-prepare VERSION=1.0.0  # Tag release
git push origin v1.0.0          # Trigger CI (GitHub Actions)
# Then: GitHub UI → Publish draft release

# For CI/CD (Automatic)
# ====================
# Triggered by: git push origin v*
# Workflow: test → build → package → create release
# Result: Draft GitHub Release ready to publish
```

---

## See Also

- [GoReleaser Documentation](https://goreleaser.com)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Semantic Versioning](https://semver.org)
- [Keep a Changelog](https://keepachangelog.com)
