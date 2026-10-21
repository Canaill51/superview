# Un tube qui coupe la sortie d'un outil le fait échouer

- **Date** : 2026-10-14
- **Domaine** : CI (PowerShell, bash), `release.yml`
- **Gravité** : perte de temps (faux échec de la release)

## Symptôme
`ffmpeg version 8.1.1-essentials_build-www.gyan.dev …` s'affiche, puis
`Write-Error: the installed ffmpeg.exe does not run` : l'étape teste `$LASTEXITCODE` après
`ffmpeg.exe -version | Select-Object -First 1`.

## Cause réelle
`Select-Object -First 1` (PowerShell) et `head -n 1` (bash) ferment le tube après une ligne. L'outil
écrit alors dans un tube fermé et sort avec un code non nul (141 sous bash avec `pipefail`).
Vérifié : le même outil, sortie capturée en entier, rend 0.

## Solution
Capturer toute la sortie, puis n'en garder que la première ligne : `$out = & "$dir\ffmpeg.exe" -version`
puis `$out | Select-Object -First 1` (Windows) ; vers un fichier puis `head -n 1 fichier` (Linux).
Voir `.github/workflows/release.yml`, étapes « The installer must install… » et « The AppImage must carry… ».

## Prévention
- [x] Test ajouté : `workflows_do_not_cut_a_tools_output_short` dans `rust/crates/superview-app/src/repo_guards.rs` (d'abord `cmd/superview/workflow_pipes_test.go`, retiré avec Go, DEC-49) — rouge si le motif réapparaît dans `.github/workflows/*.yml` (contre-preuve faite)
- [x] Leçon L-115 dans `docs/LECONS.md` § 2, ligne dans `docs/PIEGES.md`
