# Guide de Test Interactif de la GUI - Étape 12 Validation

## ✅ État Actuel
- ✅ GUI compilée avec succès (13MB)
- ✅ X11 Display disponible (DISPLAY=:1)
- ✅ ffmpeg 6.1.1 installé et fonctionnel
- ✅ Vidéo de test créée : `/tmp/test_video.mp4` (100KB, 3 secondes, 1920x1080)
- ✅ Encodage CLI validé avec succès

## Test Video Disponible

```bash
# Fichier : /tmp/test_video.mp4
# Durée : 3 secondes
# Résolution : 1920x1080
# Format : H.264 + AAC audio
# Taille : 100KB
```

## 📋 Instructions de Test Interactif de la GUI

### Méthode 1 : Test Manuel Depuis Terminal

```bash
# 1. Naviguez vers le répertoire du projet
cd /home/cedric/Documents/Github/superview

# 2. Lancez la GUI (elle s'afficheras sur le display X11)
./superview-gui

# 3. Dans la fenêtre GUI qui s'ouvrira :
#    - Cliquez sur "Select Input Video"
#    - Naviguez vers : /tmp/test_video.mp4
#    - Sélectionnez le fichier
#    - Cliquez sur "Start Encoding"
#    - Observez la barre de progression
#    - Attendez que l'encodage se termine (~3-5 secondes)
#    - Le fichier de sortie : /tmp/test_output_gui.mp4

# 4. Vérifiez le résultat
ffprobe -show_entries format=duration,size -of default=noprint_wrappers=1 /tmp/test_output_gui.mp4
```

### Méthode 2 : Test via Script Automatisé

```bash
#!/bin/bash
# test-gui.sh - Script de test GUI automatisé

cd /home/cedric/Documents/Github/superview

# Lancer la GUI et attendre 30 secondes max
echo "Démarrage de la GUI pour test..."
timeout 30 ./superview-gui &
GUI_PID=$!

# Attendre que la GUI se lance
sleep 2

# Afficher les infos de processus
echo "État du processus GUI :"
ps aux | grep superview-gui | grep -v grep || echo "Process terminé"

# Attendre la fin du processus
wait $GUI_PID 2>/dev/null

echo "Test GUI terminé"
```

## 🔍 Cas de Test Recommandés

### Test 1 : Encodage Basique
- **Fichier d'entrée** : `/tmp/test_video.mp4`
- **Options** : Défaut (H.264, bitrate auto)
- **Résultat attendu** : Fichier MP4 valide généré

### Test 2 : Sélection d'Encodeur
- **Encodeur sélectionné** : `libx265` (si disponible)
- **Fichier d'entrée** : `/tmp/test_video.mp4`
- **Résultat attendu** : Encodage HEVC réussi

### Test 3 : Bitrate Personnalisé
- **Bitrate** : `500000` (500k bytes/sec)
- **Fichier d'entrée** : `/tmp/test_video.mp4`
- **Résultat attendu** : Fichier avec un bitrate inférieur

## ✨ Points à Valider dans la GUI

- [x] La fenêtre s'ouvre sans erreurs
- [ ] Le sélecteur de fichier fonctionne
- [ ] La vidéo est chargée correctement
- [ ] Les encodeurs disponibles sont affichés
- [ ] La barre de progression se met à jour
- [ ] Le fichier de sortie est généré
- [ ] Pas de crash ou d'erreur lors de l'encodage

## 📊 Vérification des Fichiers de Sortie

Après chaque test, vérifiez le fichier généré :

```bash
# Lister les fichiers de sortie
ls -lh /tmp/test_output*.mp4

# Vérifier les propertés du fichier
ffprobe -show_entries format -of json /tmp/test_output_gui.mp4 | jq '.format'

# Jouer le fichier (si player graphique disponible)
ffplay /tmp/test_output_gui.mp4
```

## 🐛 Points de Débogage si Problèmes

Si la GUI ne fonctionne pas :

```bash
# 1. Vérifier les dépendances X11
echo "Display: $DISPLAY"
xhost +
ps aux | grep X

# 2. Vérifier les dépendances de compilation
ldd ./superview-gui | grep -E "not found|=>"

# 3. Lancer avec output verbose
./superview-gui 2>&1 | tee gui-debug.log

# 4. Vérifier les fichiers temporaires créés
ls -la /tmp/superview-session-*/ || echo "Pas de session trouvée"
```

## Status Actuel vs Attendu

| Composant | Status | Evidence |
|-----------|--------|----------|
| Go Installation | ✅ Ready | Go 1.22.2 |
| ffmpeg | ✅ Ready | v6.1.1-3ubuntu5 |
| GUI Compilation | ✅ Complete | 13MB binary created |
| Test Video | ✅ Created | /tmp/test_video.mp4 (100KB) |
| CLI Encoding | ✅ Validated | 3s video encoded successfully |
| X11 Display | ✅ Available | DISPLAY=:1 active |
| GUI Launch | ✅ Successful | Binary executable, launches cleanly |
| **GUI Interactive Test** | ⏳ PENDING | Ready for manual execution |

## 📝 Prochaines Étapes Après Validation

1. **✅ Étape 12 Complète** : Distribution CLI + GUI automatisée
2. **->** Étape 13 : Observabilité & Monitoring
   - Métriques de performance d'encodage
   - Logging structuré avancé
   - Profiling des ressources
3. **->** Étape 14 : Extensibilité
   - HTTP API pour encodage
   - Système de plugins
   - Architecture extensible

## Notes Techniques

- La GUI utilise Fyne avec OpenGL (accélération matérielle)
- Les fichiers temporaires sont isolés dans `/tmp/superview-session-{sessionID}/`
- L'encodage GUI s'exécute dans une goroutine pour garder l'interface réactive
- La barre de progression est mise à jour en temps réel

---
**Commit référence** : 25ce366 (GUI distribution complete)
**Date créé** : 2026-03-04
**Status Étape 12** : Phase de validation interactif
