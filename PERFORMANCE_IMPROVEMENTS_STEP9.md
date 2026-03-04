# Étape 9 - Performance & Optimisation

## Résumé des Améliorations

Cette étape optimise les points chauds identifiés par benchmarking, particulièrement la génération des filtres PGM pour le remapping vidéo.

## Goulots Identifiés & Solutions

### 1. **GeneratePGM - Allocation Excessive de Mémoire**

**Problème Original:**
- Utilisation de `bufio.NewWriter` avec `WriteString` pour chaque pixel
- Création de nombreuses allocations mineures (~1 820 allocations par ligne)
- ~78 000 ns/op pour générer une ligne 1920px

**Benchmark:**
```
Current (bufio.WriteString):      78,834 ns/op  + 48,736 B/op allocations
Optimized (AppendInt):            42,029 ns/op  + 0 B/op allocations
```

**Amélioration: -47% temps, -100% allocations**

**Solution Implantée:**
- Pré-allocation de buffers `[]byte` avec capacité calculée (outX * 8 bytes)
- Utilisation de `strconv.AppendInt()` au lieu de `strconv.Itoa()` + `WriteString()`
- Réutilisation des buffers ligne par ligne (reset avec `buf[:0]`)
- Direct `fX.Write(buf)` au lieu de buffered writer

### 2. **Formatage d'Entiers en Chaînes**

**Benchmark Comparatif:**
```
strconv.Itoa():                38,016 ns/op  + 6,561 B/op allocations
strconv.FormatInt():           38,114 ns/op  + 6,561 B/op allocations  
AppendInt + preallocated buf:  13,476 ns/op  + 0 B/op allocations
```

**Amélioration: -65% temps, zéro allocation**

**Recommandation:** Utiliser `strconv.AppendInt()` quand on construit des chaînes dans des boucles critiques.

### 3. **Gestion des Buffer Flush**

**Benchmark:**
```
Flush par ligne (1000x):       41,312 ms/op  + 7.1 MB allocs
Batch flush (1x):             41,998 ms/op  + 23.6 MB allocs
```

**Conclusion:** Pas de différence significative temps-wise, mais notre approche directe avec `fX.Write()` évite le buffering superflu.

## Benchmarks Créés

Fichier: `common/common_bench_test.go`

### Tests de Référence:
- `BenchmarkGeneratePGMMapCalculation`: Mesure le coût pur des calculs mathématiques
- `BenchmarkStringFormatting`: Compare formatage itoa vs AppendInt
- `BenchmarkLineGeneration`: Compare génération de lignes PGM (ancien vs nouveau)
- `BenchmarkBufferFlush`: Analyse stratégies de flush

### Exécution:
```bash
go test -bench=. ./common -benchmem -run=^$
```

## Impact en Conditions Réelles

Pour une vidéo 1080p (1920×1080, squeeze mode):
- **Ancien:** ~78,000 ns/line × 1,080 lines ≈ **84.2 ms**
- **Nouveau:** ~42,000 ns/line × 1,080 lines ≈ **45.4 ms**
- **Gain:** **38.8 ms** (~46% plus rapide)

Pour 4K (3840×2160):
- **Ancien:** ~300+ ms pour générer les filtres
- **Nouveau:** ~160+ ms pour générer les filtres
- **Gain:** ~47% hors les allocations réduites réduisent la pression GC

## Code Source Modified

**common/common.go - Fonction GeneratePGM (lignes 339-458):**

Changements clés:
1. Suppression de `bufio.NewWriter` et `bufio.Flush()`
2. Pré-allocation: `bufXCapacity := outX * 8`
3. Boucle ligne par ligne avec réinitialisation: `bufX = bufX[:0]`
4. Utilisation exclusive de `strconv.AppendInt()` et `append()`
5. Écriture directe: `fX.Write(bufX)` au lieu de writer buffered

## Tests d'Intégrité

✅ **Compilation:** Binaires CLI & GUI compilent sans erreur
✅ **Tests:** Tous 32 tests passent (26.6% couverture)
✅ **Fonctionnalité:** GeneratePGM produit les mêmes fichiers PGM
✅ **Performance:** Benchmarks -47% temps pour générer PGM

## Recommandations Futures

### Courte Terme (Étape 10):
- ✅ **Vérifier mémoire:** Profiler avec `pprof` sur vidéos réelles 4K/8K
- Optimiser `CheckFfmpeg()` avec cache encodeurs
- Paralléliser calculs mathématiques si CPU-bound

### Moyen Terme:
- SSE/AVX intrinsics pour calculs de offset (math.Pow est coûteux)
- Memory pool pattern pour buffers (réduire allocations encore plus)
- Lazy encoding - démarrer ffmpeg sans générer tous les PGM d'avance

### Long Terme:
- Implémentation GPU du remapping (CUDA/OpenCL)
- Caching des PGM pour vidéos avec mêmes dimensions

## Fichiers Modifiés

| File | Changes | LOC |
|------|---------|-----|
| common/common.go | Fonction GeneratePGM optimisée | +40 |
| common/common_bench_test.go | 4 nouveaux benchmarks | +150 |

## Commande de Test

```bash
# Compiler et tester
go build superview-cli.go && go build superview-gui.go

# Benchmarks détaillés
go test -bench=. ./common -benchmem -run=^$

# Tests d'intégrité
go test ./common -cover -v
```
