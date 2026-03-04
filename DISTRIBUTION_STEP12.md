# Étape 12: Distribution & Versions Officielles — Documentation

## Overview

Étape 12 met en place un système d'automatisation complet pour les releases officielles avec:
- **CLI:** Cross-compilation via GoReleaser (tous les platforms d'une seule machine)
- **GUI:** Compilation native par plateforme (macOS sur macOS, Windows sur Windows, Linux sur Linux)
- Distribution multiplateforme, archivage automatique, checksums

**Status:** ✅ Complete
**Commits Affected:** Configuration files for GoReleaser and GitHub Actions workflows

---

## Architecture

### Release Workflow Multi-Plateforme

```
Developer Tags Release (git push origin vX.Y.Z)
    ↓
GitHub Actions Triggered
    ├─ Test Suite (Ubuntu) → Coverage Gate ✓
    │
    ├─ Build CLI (Ubuntu) → GoReleaser
    │   └─ Cross-compile CLI: Linux/macOS/Windows (amd64, 386, arm64)
    │
    ├─ Build GUI Linux (Ubuntu)
    │   └─ Native compile: Linux (amd64, 386, arm64)
    │
    ├─ Build GUI macOS (macOS)
    │   └─ Native compile: macOS (amd64, arm64) - 2 binaires
    │
    ├─ Build GUI Windows (Windows)
    │   └─ Native compile: Windows (amd64, 386)
    │
    └─ Create Release (Ubuntu) → Aggregate + Checksums
        ↓
        GitHub Release (Draft Mode)
        ↓
Maintainer Publishes (Manual)
        ↓
Users Download Multiplateforme
```

### Stratégie de Compilation

| Composant | Approche | Raison |
|-----------|----------|--------|
| **CLI** | GoReleaser (cross-compile) | Pure Go, aucune dépendance native |
| **GUI Linux** | Native build (Ubuntu) | Dépend de libGL, libXcursor, libXrandr |
| **GUI macOS** | Native build (macOS) | Dépend de Xcode/Cocoa frameworks |
| **GUI Windows** | Native build (Windows) | Dépend de MinGW/MSVC, DirectX |

---

## Components

### 1. GoReleaser Configuration (`.goreleaser.yml`)

**Purpose:** Cross-platform CLI compilation via GoReleaser

**Scope:** CLI Binaries ONLY
- Pure Go code → cross-compile d'une unique machine (Ubuntu)
- Windows, macOS, Linux en une seule étape

**Build Targets:**

```
CLI Multiplateforme:
├─ Linux: amd64, 386, arm64
├─ macOS: amd64 (Intel), arm64 (Apple Silicon)
└─ Windows: amd64, 386
```

**Archives Créées:**
```
superview-1.0.0-Linux-x86_64.tar.gz
superview-1.0.0-Linux-i386.tar.gz
superview-1.0.0-Linux-aarch64.tar.gz
superview-1.0.0-Darwin-x86_64.zip
superview-1.0.0-Darwin-aarch64.zip
superview-1.0.0-Windows-x86_64.zip
superview-1.0.0-Windows-i386.zip
```

**Note:** GUI **NOT** compilé par GoReleaser (dépendances natives → compilation native par OS)

---

### 2. GitHub Actions Release Workflow (`.github/workflows/release.yml`)

**Purpose:** Automate entire release process via parallel platform-specific builds

**Jobs (Exécutés en parallèle après tests):**

#### a) Test Job (Ubuntu)
- Runs before any build
- **Coverage Gate:** Enforces 30% minimum
- Fails fast if tests don't pass
- Prevents broken releases

#### b) Build CLI Job (Ubuntu)
- Runs GoReleaser for cross-platform compilation
- Creates CLI archives pour tous les platforms
- Génère checksums.txt (SHA256)
- **Output:** 7 archives CLI (Linux/macOS/Windows)

#### c) Build GUI Linux Job (Ubuntu)
- Native compilation sur Ubuntu
- Install GUI dependencies (libgl1-mesa-dev, etc.)
- Compile pour: amd64, 386, arm64
- **Output:** 3 archives tarball GUI

#### d) Build GUI macOS Job (macOS Runner)
- Runs on macOS native hardware
- Compile both: amd64 (Intel) + arm64 (Apple Silicon)
- **Output:** 2 archives ZIP GUI

#### e) Build GUI Windows Job (Windows Runner)
- Runs on Windows native hardware
- Compile both: amd64 + 386
- **Output:** 2 archives ZIP GUI

#### f) Create Release Job (Ubuntu)
- Runs after all build jobs complete
- Aggregate tous les artifacts
- Calculate combined checksums
- Create GitHub Release (Draft mode)
- Generate release summary

#### g) Notify Job (Ubuntu)
- Final status report
- Confirms all builds completed

**Parallelization:**
```
┌─ Test (required first)
├─ Build CLI (parallel) ─────┐
├─ Build GUI Linux (parallel) ├─ Create Release ─ Notify
├─ Build GUI macOS (parallel) │
└─ Build GUI Windows (parallel) ┘
```

**Total Distributed Artifacts:**
```
✅ 7 CLI archives (all platforms)
✅ 3 GUI archives (Linux: amd64, 386, arm64)
✅ 2 GUI archives (macOS: Intel + ARM)
✅ 2 GUI archives (Windows: x86_64, x86)
───────────────────────────────────────
📦 Total: 14 application archives + 1 checksums.txt
```

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
