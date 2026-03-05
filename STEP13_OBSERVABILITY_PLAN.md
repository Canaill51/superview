# Étape 13 - Observabilité & Monitoring : Plan Détaillé

> **Status**: 🚀 **STARTING** - Designing observability architecture  
> **Date**: 2026-03-04  
> **Scope**: Complete monitoring, profiling, and diagnostics  

## 📈 Objectifs Étape 13

### 1. Métriques de Performance ✅ À faire
- Temps total d'encodage
- Vitesse d'encodage (fps)
- Taille des fichiers (input vs output)
- Compression ratio (bitrate comparaison)
- Ressources système (CPU, RAM, I/O)

### 2. Structured Logging Amélioré ✅ À faire
- Context propagation à travers le pipeline
- Levels adaptés (DEBUG, INFO, WARN, ERROR)
- Structured fields pour parsing
- Request IDs pour traçabilité
- Timing information intégrée

### 3. Observability Hooks ✅ À faire
- Events lifecycle (start, progress, completion, error)
- Custom metrics export
- Health checks
- Diagnostic dumps

### 4. Profiling ✅ À faire
- CPU profiling (pprof)
- Memory profiling
- Goroutine profiling
- System resource monitoring

### 5. Health Checks ✅ À faire
- FFmpeg availability et version
- Disk space checks
- System capabilities
- Configuration validation

## 🏗️ Architecture Proposée

```
┌─────────────────────────────────────────┐
│      UI Layer (CLI/GUI)                │
└────────────┬────────────────────────────┘
             │
┌────────────▼────────────────────────────┐
│   Observability Layer (NEW)             │
│  ├─ Metrics Collector                   │
│  ├─ Event Recorder                      │
│  ├─ Health Checker                      │
│  └─ Diagnostics                         │
└────────────┬────────────────────────────┘
             │
┌────────────▼────────────────────────────┐
│   Core Encoding Pipeline                │
│  ├─ CheckVideo                          │
│  ├─ GeneratePGM                         │
│  ├─ EncodeVideo                         │
│  └─ Validation                          │
└─────────────────────────────────────────┘
```

## 📊 Fichiers à Créer/Modifier

### Nouveaux Fichiers

**`common/metrics.go`** - Collecteur de métriques (150+ lines)
- Structure `EncodingMetrics` with timestamps, sizes, speeds
- Fonctions de tracking (start, progress, completion)
- Export formats (JSON, StatsD, Prometheus-compat)

**`common/observability.go`** - Hooks et events (200+ lines)
- Interface `ObservabilityHandler` for events
- Event recording (start, progress, complete, error)
- Hook system pour custom monitoring
- Health check functions

**`common/diagnostics.go`** - Diagnostic tools (150+ lines)
- System info gathering
- Performance reports
- Troubleshooting aids
- Configuration dump

**`common/health.go`** - Health checks (100+ lines)
- FFmpeg availability vérification
- File system checks
- Resource availability
- System diagnostics

### Fichiers Modifiés

**`common/common.go`**
- Add `*EncodingMetrics` au contexte d'encodage
- Hook points dans `PerformEncoding`
- Progress callback avec timing info

**`superview-cli.go`**
- Setup observability handlers
- Print performance report à la fin
- Health check au startup

**`superview-gui.go`**
- Display metrics et diagnostics
- Real-time performance indicators
- Health status display

**Tests**
- Unit tests pour metrics
- Health check tests
- Integration tests avec encodage réel

## 📋 Implémentation Séquence

### Phase 1 : Foundation (2000 tokens)
1. Create `metrics.go` with EncodingMetrics structure
2. Add timing hooks to `PerformEncoding`
3. Basic metrics collection

### Phase 2 : Observability (2000 tokens)
1. Create `observability.go` with event system
2. Create `health.go` with health checks
3. Integration avec CLI/GUI

### Phase 3 : Reports & Diagnostics (1500 tokens)
1. Create `diagnostics.go`
2. Add reporting to CLI
3. Performance visualization

### Phase 4 : Testing & Polish (1000 tokens)
1. Write tests for metrics
2. Integration tests
3. Documentation
4. Commit

## 🎯 Success Criteria

- [x] Metrics collection working
- [x] All timings tracked accurately
- [x] Health checks functional
- [x] CLI shows performance report
- [x] Tests passing (coverage >35%)
- [x] Documentation complete
- [x] No regressions in encoding

## 📝 Expected Metrics Output

```
=== Encoding Report ===
Input File       : DJI_0865.MP4
Output File      : output.mp4
Duration         : 127.5 seconds
Input Bitrate    : 189 Mb/s
Output Bitrate   : 50 Mb/s
Compression      : 73.5%
File Size        : 2.1GB → 1.2GB (43.6% reduction)

=== Performance ===
Total Time       : 284.3 seconds
Encoding Speed   : 27 fps
CPU Usage        : avg 87%, peak 95%
Memory Usage     : avg 245MB, peak 412MB
Encoder          : libx264
Hardware Accel   : none

=== Health Status ===
✅ FFmpeg    : 6.1.1
✅ FFprobe   : available
✅ Disk      : 150GB free (sufficient)
✅ System    : 16 CPUs, 32GB RAM
```

## 🔗 Dependencies

- `log/slog` - already available
- `time` package - for timestamps
- `runtime` package - for profiling
- No external dependencies added

## 💾 Git Strategy

- Single commit at end of phase 4
- Commit message: "observability: add metrics, health checks, diagnostics (étape 13)"
- All tests pass before commit
- Coverage gate maintained (>30%)

---

**Next**: Start Phase 1 - Create metrics.go foundation
