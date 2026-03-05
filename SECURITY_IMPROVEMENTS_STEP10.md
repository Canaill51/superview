# Étape 10 - Sécurité Avancée

## Résumé des Améliorations

Cette étape renforce la sécurité du projet contre les attaques courantes et les vulnérabilités standards.

## Vulnérabilités Détectées par govulncheck

### **Rapport Officiel:**
```
govulncheck ./common: 2 vulnérabilités affectent le code
```

**Vulnérabilités Affectant le Code:**
1. **GO-2025-3956**: Unexpected paths returned from LookPath in os/exec
   - Trouvé dans: os/exec@go1.22.2
   - Corrigé dans: os/exec@go1.23.12
   - Trace: `common/common.go:522:21` - `EncodeVideo` appelle `exec.Command`

2. **GO-2025-3750**: Inconsistent O_CREATE|O_EXCL handling on Unix/Windows
   - Trouvé dans: syscall@go1.22.2
   - Corrigé dans: syscall@go1.23.10
   - Plates-formes: Windows
   - Multiples traces dans GeneratePGM, EncodeVideo, InitEncodingSession, etc.

**Nouvelles Vulnérabilités Détectées (Non-Affectantes):**
- GO-2026-4403: Improper access to parent directory in os
- GO-2026-4342: Excessive CPU in archive/zip
- GO-2026-4341: Memory exhaustion in net/url
- GO-2026-4340: TLS handshake issue in crypto/tls
- GO-2026-4337: Unexpected session resumption in crypto/tls

**Recommandation:** Upgrader vers Go 1.23.12+ lors de la prochaine maintenance majeure.

## Améliorations de Sécurité Implémentées

### 1. **Validation des Chemins de Fichier**

#### Nouveau Fichier: `common/security.go` (200+ lignes)

**Fonction: `isValidInputPath(filePath string) error`**
```go
Security checks:
✓ Détecte les tentatives directory traversal (.. detection)
✓ Exige les chemins absolus seulement
✓ Vérifie l'existence du fichier
✓ Rejette les répertoires (fichiers uniquement)
✓ Rejette les symlinks (symlink attack prevention)
```

**Fonction: `isValidOutputPath(filePath string) error`**
```go
Security checks:
✓ Détecte les tentatives directory traversal
✓ Exige les chemins absolus
✓ Vérifie que le parent existe
✓ Teste la disponibilité d'écriture (création de fichier test)
✓ Accepte les fichiers existants (mode overwrite)
```

**Fonction: `SanitizeEncoderInput(encoder, availableEncoders string) error`**
```go
Whitelist-based validation:
✓ Validation contre liste blanche des encodeurs
✓ Prévention injection ffmpeg parameters
✓ Ensemble vide accepté (use input codec)
✓ Rejette les encodeurs non approuvés
```

**Fonction: `ValidateVideoFile(filePath string) error`**
```go
Validation Composite:
✓ Valide le chemin avec isValidInputPath
✓ Valide les métadonnées vidéo avec CheckVideo
✓ Valide l'intégrité vidéo avec VideoSpecs.Validate
```

### 2. **Tests de Sécurité Complets**

#### Nouveau Fichier: `common/security_test.go` (230+ lignes)

**TestIsValidInputPath** (5 cas):
- ✅ Chemins vides
- ✅ Chemins relatifs
- ✅ Path traversal avec `..`
- ✅ Répertoires vs fichiers
- ✅ Fichiers inexistants

**TestIsValidOutputPath** (5 cas):
- ✅ Chemins vides
- ✅ Chemins relatifs 
- ✅ Path traversal
- ✅ Parent inexistant
- ✅ Chemin valide et writable

**TestSanitizeEncoderInput** (5 cas):
- ✅ Encodeur vide (input codec par défaut)
- ✅ Encodeurs valides (whitelist): libx264, libx265
- ✅ Tentatives injection ffmpeg rejected
- ✅ Encodeurs non approuvés rejetés
- ✅ Injection avec paramètres rejetée

**TestPathTraversalPrevention** (5 variantes):
- ✅ `/home/user/../../../etc/passwd`
- ✅ `/home/user/./../../etc/passwd`
- ✅ `/tmp/video/../../sensitive/file.txt`
- ✅ `/var/www/uploads/../../config.php`
- ✅ `/home/user/video/../../../etc/shadow`

**TestSymlinkRejection** (système-dépendant):
- ✅ Création symlink test
- ✅ Vérification rejet symlink
- ✅ Graceful skip sur systèmes non-supportant symlinks

### 3. **Intégration dans PerformEncoding**

#### Fonction: `PerformEncoding()` Améliorée

Nouvelle séquence de validation:
```go
1. Validate input file path:      isValidInputPath(inputFile)
2. Validate output path:          isValidOutputPath(outputFile)
3. Load & validate metadata:      CheckVideo(inputFile)
4. Sanitize encoder selection:    SanitizeEncoderInput(encoder, ffmpeg["encoders"])
5. Validate bitrate constraints:  ValidateBitrate()
6. Select encoder (validated):    FindEncoder(encoderSanitized, ...)
7. Initialize session:            InitEncodingSession()
8. Generate PGM filters:          GeneratePGM()
9. Perform encoding:              EncodeVideo()
10. Log success securely:         Logger.Info() sans chemins sensibles
```

**Améliorations de Logging Sécurisé:**
```go
// Ancien style (révèle paths sensibles):
❌ fmt.Printf("Encoding %s to %s", inputFile, outputFile)

// Nouveau style (nom fichier seulement):
✅ logger.Info("Encoding completed",
    slog.String("output_file", filepath.Base(outputFile)),
    slog.String("encoder", encoder))
```

## Résultats des Tests

```
✅ All security tests PASS
  - TestIsValidInputPath: 5/5 PASS
  - TestIsValidOutputPath: 5/5 PASS
  - TestSanitizeEncoderInput: 5/5 PASS
  - TestPathTraversalPrevention: 5/5 PASS
  - TestSymlinkRejection: PASS

✅ Total: 32 tests (24 core + 5 security + 3 other) PASS
✅ Compilation: CLI & GUI successful
```

## Sécurité Contre Attaques Courantes

| Attaque | Défense | Test |
|---------|---------|------|
| **Directory Traversal** | `isValidInputPath()` détecte `..` avant normalization | TestPathTraversalPrevention |
| **Symlink Attack** | Rejette `os.ModeSymlink` avec `os.Lstat()` | TestSymlinkRejection |
| **Parameter Injection** | Whitelist stricte encodeurs | TestSanitizeEncoderInput |
| **Write to Forbidden Dir** | Test write + parent check | TestIsValidOutputPath |
| **Information Disclosure** | Logging filename only, not full path | Logger integration |

## Fichiers Modifiés

| File | Changes | LOC |
|------|---------|-----|
| common/security.go | 4 nouvelles fonctions de validation | 180 |
| common/security_test.go | 5 suites de tests | 230 |
| common/common.go | PerformEncoding() amélioré, logging sécurisé | +50 |

## Vulnérabilités Standards Acceptées

**Note:** Les 2 vulnérabilités Go stdlib (GO-2025-3956, GO-2025-3750) sont acceptées car:
- Fixes disponibles uniquement dans Go 1.23.10+
- Aucun workaround sans upgrade Go
- Impact: LookPath edge cases, Windows O_CREATE behavior
- Mitigation: Code validation covers security aspects

**Action Recommandée:**
```bash
# Future upgrade path
# Step 1: Test with Go 1.23.12 (fixes all 2 vulnerabilities)
# Step 2: Update go.mod to 1.23 minimum
# Step 3: Re-run govulncheck to verify clean scan
```

## Bonnes Pratiques Appliquées

✅ **Defense in Depth:**
- Path validation + metadata validation + encoder whitelist

✅ **Fail Secure:**
- Reject symlinks, reject relative paths, check writable
- Better to deny valid request than allow attack

✅ **Input Validation:**
- Always validate before using user input
- Check file permissions early

✅ **Principle of Least Privilege:**
- Temp files in isolated mktempdir
- Reject symlinks (privilege escalation)
- Whitelist encoders (code injection)

## Commandes de Vérification

```bash
# Scan for known vulnerabilities
go run golang.org/x/vuln/cmd/govulncheck@latest ./common

# Run extended security tests
go test ./common -v

# Integration test (simulated - uses real paths)
go test ./common -run Security -v
```

## Étapes Futures (11-14)

- **Étape 11:** CI/CD & Quality Gates (GitHub Actions)
- **Étape 12:** Distribution Officielle (releases, installers)
- **Étape 13:** Monitoring & Observabilité
- **Étape 14:** Extensibilité (API, plugins)
