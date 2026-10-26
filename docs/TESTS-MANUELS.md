# Tests manuels du binaire Rust

> Journal interne (français). Ce que les tests automatiques ne peuvent pas voir : un vrai
> bureau, un vrai GPU, un vrai sélecteur de fichiers. À passer à la main, sur un binaire
> construit comme celui qui sera livré, avant chaque version (la première fut RC-0.3.0, puis
> v0.3.0, publiée le 2026-10-20). Cocher et
> dater ; noter la machine. Une case non cochée n'est pas un échec : c'est une inconnue.

## Passe du 2026-10-19 (AppImage de l'essai à blanc, master à 9a39121)

Portable de l'utilisateur : KDE, Intel Tiger Lake UHD (VAAPI en CQP seul, HuC absent) + RTX 3050 Ti.
Cochées : celles que l'utilisateur a passées à la main dans la fenêtre (clips 4:3 de 20 s et de 5 min).
**Source 10 bits** et **Plusieurs pistes audio** ont été mesurées avec `superview-harness` (sortie HEVC
`yuv420p10le` ; vidéo + deux pistes AAC), pas dans la fenêtre. L'annulation par SIGINT/SIGTERM a aussi
été mesurée avec le banc : aucun `ffmpeg`, sortie partielle ou dossier `superview-*` ne reste.
**Non faites, donc non cochées** : portail lent ou sans backend, « Linux sans rien » (champ texte), VAAPI et « GPU Intel seul » dans la
fenêtre (Diagnostic et conversion par le banc vérifiés : `h264_vaapi` et `hevc_vaapi` produisent un
MP4 1920x1080 ; la sortie de 4 Mb/s demandés pèse ~9,8 Mb/s sur une mire, le débit n'est pas suivi),
lecture complète du Diagnostic, lien d'installation, encodeurs AMD et QSV.

**Windows (même jour, double démarrage du même portable, installateur de l'essai à blanc, empreinte
SHA-256 vérifiée)** : installation (SmartScreen puis « Exécuter quand même »), lancement, titre avec la
version, icône dans la barre de titre et la barre des tâches, sélecteur natif, Diagnostic, conversion,
Cancel et fermeture avec la croix (aucun `ffmpeg.exe`, sortie partielle ou dossier `superview-*` ne reste),
désinstallation : tout conforme, d'après l'utilisateur. Seul le sélecteur natif a une case propre ; la case
**Icône de la fenêtre** reste ouverte parce que la partie Linux (KDE) n'a pas été dite vue.

**Suite du 2026-10-20** (KDE sur Wayland) : **Icône de la fenêtre** cochée. Sous Wayland, KDE ne lit pas l'icône
dans la fenêtre : il la cherche dans un `.desktop` nommé d'après l'identifiant de la fenêtre (`com.canaill51.superview` alors, `superview` depuis DEC-64). Une
AppImage lancée sans intégration au bureau montre donc une icône générique ; avec un `.desktop` pointant sur
elle (ce que fait AppImageLauncher), barre de titre et barre des tâches montrent la bonne icône. Rien à changer
dans le code. **Diagnostic : lire tout le rapport** cochée (la partie Diagnostic, d'après l'utilisateur).
**En suspend, volontairement** : portail lent ou sans backend, et champ texte de repli (« Linux sans rien »),
jugés peu probables sur un bureau ordinaire ; encodeurs AMD et QSV, en attente de matériel AMD et Intel
récents ; lien d'installation (voir sa case).

Préparer un clip : `ffmpeg -f lavfi -i testsrc2=size=1440x1080:rate=30 -f lavfi -i sine -t 10 -c:v libx264 -c:a aac clip.mp4`

## Conversion

- [x] **Seconde conversion** : convertir un clip, puis, sans fermer la fenêtre, en choisir un
      autre et convertir encore. Le bouton Start revient, la barre repart de zéro, la
      ligne « Hardware » est celle du nouveau fichier.
- [x] **Annulation immédiate** : appuyer sur Cancel dès que Start est pris en compte. Aucun
      processus `ffmpeg` ne reste (`pgrep ffmpeg`), aucun fichier de sortie partiel, aucun
      dossier `superview-*` dans le dossier temporaire du système.
- [x] **Source 10 bits** : `-pix_fmt yuv420p10le -c:v libx265` ; la conversion réussit et la
      sortie garde le 10 bits (`ffprobe`).
- [x] **Plusieurs pistes audio** : un clip avec deux pistes ; les deux sont dans la sortie.
- [x] **Fermeture pendant l'encodage** : croix de la fenêtre (et Quit) pendant une conversion :
      la question « Cancel and quit » s'affiche ; Yes arrête tout sans rien laisser derrière
      (mêmes vérifications que l'annulation) ; No laisse la conversion continuer.
- [x] **SIGINT / SIGTERM sur la fenêtre** : lancer la conversion, puis `kill -INT <pid>` et
      `kill -TERM <pid>` (deux essais) : la fenêtre se ferme, la conversion se déroule, rien ne
      reste. Le trajet signal → fin de boucle n'est pas couvert par un test de fenêtre.

## Sélecteurs de fichiers (PR 6.4)

- [x] **Windows** : « Choose input file » et « Choose output file » ouvrent la boîte native ;
      le filtre n'offre que les MP4 ; la sortie propose `output.mp4` ; un nom sans `.mp4`
      reçoit `.mp4` ; annuler ne change rien. Aucun repli n'existe sous Windows (DEC-43).
- [x] **Linux avec portail** (GNOME, ou KDE avec `xdg-desktop-portal-kde`) : mêmes vérifications.
      Annuler vite (Échap tout de suite) ouvre peut-être le sélecteur de repli : c'est le prix
      du seuil de 300 ms, noter si cela gêne.
- [x] **Linux sans portail, avec kdialog** (la machine KDE de l'utilisateur) : lancer avec
      `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` et sans `zenity` dans le `PATH` : la boîte
      de kdialog s'ouvre, avec le filtre MP4 et `output.mp4`.
- [ ] **Portail sans backend ou lent à démarrer** : sur une session où `xdg-desktop-portal` tourne
      sans backend FileChooser, ou juste après l'ouverture de session (démarrage à froid du portail,
      plus de 300 ms), le bouton peut ne rien faire : c'est le mode d'échec connu (DEC-43, pris pour
      une annulation). Noter si cela arrive en pratique ; au premier clic rapide en échec, les
      suivants vont directement à zenity/kdialog/champ texte.
- [ ] **Linux sans rien** (ni portail, ni zenity, ni kdialog) : un `PATH` qui ne contient que
      `ffmpeg` et `ffprobe` : le champ texte s'ouvre dans la fenêtre. Vide : « Please type the path
      of a file. » ; relatif : le message sur le chemin complet ; un chemin complet valide
      s'applique ; Cancel ne change rien et rend le bouton.
- [x] **Version dans la barre de titre** : « Superview » suivi de la version du tag et de la révision
      (« Superview 0.3.0 (abc1234) »), « Superview dev » pour une compilation locale.
- [x] **Icône de la fenêtre** : visible dans la barre de titre et dans la barre des tâches (Windows
      et Linux avec gestionnaire de fenêtres). Sous Xvfb sans gestionnaire, seule la propriété
      `_NET_WM_ICON` se mesure (1024x1024).

## Matériel

- [x] **NVENC** (carte NVIDIA, pilote au moins 570) : « Hardware: » annonce NVENC ; la conversion
      l'utilise (`nvidia-smi` montre le processus) ; une source que NVENC refuse retombe sur le
      processeur.
- [x] **VAAPI** (portable Intel) : idem avec VAAPI. Passé le 2026-10-20 (AppImage RC-0.3.0, portable
      Tiger Lake + RTX 3050 Ti, Nvidia masquée, voir le procédé ci-dessous).
- [x] **GPU Intel seul** (DEC-48 ; Nvidia masquée ou absente, pilote VAAPI sans firmware HuC qui
      répond `supported modes: CQP`) : le Diagnostic dit `[OK] h264_vaapi: works, constant quality
      only` ; « Hardware: » finit par « (constant quality) » ; la conversion produit un MP4 16:9
      lisible (`ffprobe` : codec h264, pas de débit demandé respecté, taille qui suit le contenu) ;
      le journal garde le refus en débit et dit que le débit demandé n'est pas utilisé.
      Passé le 2026-10-20 : Diagnostic `h264_vaapi` et `hevc_vaapi` « constant quality only » ;
      « Hardware: planned hevc_vaapi encode + VAAPI decode (constant quality) » ; source HEVC de
      DJI, donc sortie `hevc` 1920x1080 (et non h264), 64 Mb/s pour 93 Mb/s demandés, 118 s ; le
      journal dit « the requested bit rate is not used ».
      **Procédé** : la Nvidia se masque sans toucher au système avec `bwrap --dev-bind / /` et un
      `--ro-bind /dev/null` sur chaque `/dev/nvidia*` et sur le `renderD` dont le pilote est
      `nvidia` ; NVENC répond alors `CUDA_ERROR_NO_DEVICE` et la sonde retombe sur VAAPI. Une
      AppImage ne se monte pas sous `bwrap` (FUSE) : lancer avec `APPIMAGE_EXTRACT_AND_RUN=1`.
- [x] **Test « device frame »** : sur le portable Intel VAAPI, passer
      `SUPERVIEW_REQUIRE_FFMPEG=1 cargo test -p superview-core --test real_ffmpeg -- device_frame`
      (`crates/superview-core/tests/real_ffmpeg.rs`). Il saute en silence sans GPU : un
      vert sans matériel ne prouve rien. Passé le 2026-10-20 sous le masque Nvidia décrit
      ci-dessus, avec `--nocapture` : la sortie dit « using h264_vaapi », donc il a bien tourné.

## Plugin OpenFX pour DaVinci Resolve (DEC-65, DEC-66, prototype)

`sh packaging/ofx/build_bundle.sh` (Linux, ou Git Bash sous Windows avec les Build Tools, `docs/ENVIRONNEMENT.md`)
ou l'artefact `Superview.ofx.bundle-Windows` de la CI. Passés le 2026-10-25 sous Windows 11, Resolve 21.1.1
**gratuit**, i7-11800H et RTX 3050 Ti Laptop (4 Go, pilote 616.92, CUDA) ; le chemin CUDA a tourné (compté dans
un chronométrage temporaire, retiré depuis).

- [x] **Chargement** : copier `Superview.ofx.bundle` dans `/usr/OFX/Plugins` (Linux, sudo) ou
      `C:\Program Files\Common Files\OFX\Plugins` (Windows), redémarrer Resolve : *Effets → OpenFX → Superview*
      apparaît. Passé avec Free ; Studio non essayé.
- [x] **Élargir** : *Mismatched resolution* = **Scale entire image to fit**, un clip 4:3 sur la timeline, l'effet
      sur le clip (page Montage) : le 4:3 remplit le 16:9, le centre garde son rapport, pas de bande. Passé en
      timeline 3840×2160 avec `DJI_0221.MP4` (2688×2016, H.264, 59,94 i/s).
- [x] **Parité** : exporter la timeline et comparer à la même vidéo convertie par Superview. Passé : export 4K
      du plugin contre la sortie de l'application (3584×2016, agrandie en 3840×2160 pour comparer) : 38 à 41 dB
      de PSNR aux images 10, 60, 120 et 170 s, contre 19 à 25 dB pour un simple étirement de la source ; bords
      de même luminosité. Une source 59,94 i/s dans une timeline 29,97 : comparer à l'image voisine de 1/60 s.
- [x] **Parité au pixel** (2026-10-26, Lot 1 de `docs/PLAN-OFX.md`) : `DJI_0221.MP4` réduite en 1440×1080
      29,97 i/s (10 s, x264 CRF 8), convertie par l'application (`superview-harness convert`, libx264) et par
      le plugin dans une timeline 1920×1080. Export H.264 : 47,4 dB, **uniformes** (bords 47,9 et 48,9 dB,
      centre 46,8 dB) ; colonnes de bord égales à l'application (107 et 126). Un écart de géométrie dégraderait
      d'abord les bords, où le warp agit : l'écart restant est la compression de l'export, pas le calcul.
- [x] **CUDA contre CPU dans Resolve** (2026-10-26) : *Préférences → Mémoire et GPU → OpenCL* (Resolve redémarre)
      fait passer le plugin en CPU. Les exports ne sont **pas** identiques, et ce n'est pas le plugin : un **témoin
      sans l'effet**, même timeline 1920×1080, DNxHR HQX, diffère déjà entre les deux modes (279 trames sur 300,
      102 dB, aucun pixel de luma différent aux trames 0, 150 et 299). Avec l'effet : 97 dB, même ordre (le warp
      répète les colonnes de bord, donc leurs écarts). Le plugin copie des pixels entiers et `tests/cuda.rs`
      exige l'égalité GPU/CPU : l'écart est le traitement de Resolve, qui change d'API avec le mode. Dans un
      mode donné, deux exports de Resolve sont identiques trame pour trame. Exporter en DNxHR, pas en H.264 :
      en H.264, le même écart donne 52 dB et masque tout.
- [x] **Vitesse** (CUDA) : lecture en temps réel, 29,97 i/s en timeline 4K. Avant DEC-66 : 6 à 10 i/s.
- [x] **Centrage** (2026-10-26) : timeline 1921×1080, source 1440×1080. Resolve **exporte en 1922** et ajoute à
      gauche une colonne sombre ; le témoin sans l'effet a la même (44 contre 107). Les colonnes du plugin sont
      propres des deux côtés (DNxHR : 110 à gauche, 127 à droite). Pas de défaut du plugin.
- [x] **Un-squeeze** (2026-10-26) : sans vidéo GoPro, la source étirée en 1920×1080 tient lieu de vidéo
      SuperView ; contre l'application (`convert --squeeze`) : 47,4 dB, uniformes, comme *Widen*. Une vraie
      vidéo GoPro reste à essayer quand il y en aura une.
- [x] **Rendu final** (page Livraison) : l'effet est bien appliqué à l'export, pas seulement à l'aperçu (voir
      *Parité*). Resolve calcule à la résolution de la **timeline** puis agrandit à l'export (mesuré sans l'effet :
      un export 4K d'une timeline 1080p est un 1080p agrandi) : exporter depuis une timeline à la taille voulue.
- [x] **Autres réglages** (2026-10-26) : image fausse, confirmé ; à documenter dans le README si le plugin est
      livré (Lot 6). *Stretch frame to all corners* : 19,9 dB contre l'application, moins qu'un simple étirement.
      *Center crop with no resizing* : `DJI_0221.MP4` (2688×2016) en timeline 1920×1080, le plugin reçoit un
      cadre déjà rogné, sans bandes, et l'élargit quand même. À essayer avec une source **plus grande** que la
      timeline : une source de même hauteur donne exactement l'image de *Scale entire image to fit* (mesuré).
- [x] **Défilement** (2026-10-26) : défilement rapide sur le clip, ni trame rouge ni noire ni message d'erreur.
- [ ] **Studio** et **Linux** : chargement, CUDA et vitesse, non essayés.
- [ ] **Journal** (Lot 2 de `docs/PLAN-OFX.md`) : après un démarrage de Resolve et une lecture avec l'effet,
      `%LocalAppData%\superview-ofx\superview-ofx.log` (Linux : `~/.cache/superview-ofx/superview-ofx.log`)
      existe et dit `plugin loaded`, `CUDA rendering offered to the host`, puis une seule fois
      `rendering on the GPU with CUDA (WxH)` par instance de l'effet, et rien par trame. Resolve réglé sur
      OpenCL : `CUDA rendering not offered, the host does not propose it` et `rendering on the CPU`. Le fichier
      reste petit après plusieurs minutes de lecture (le fork journalise chaque message de l'hôte, filtré).

## Divers

- [x] **Diagnostic : lire tout le rapport** : le bouton Diagnostic ouvre le rapport en liste : une
      icône ronde par contrôle, VERTE avec une coche (OK) ou ROUGE avec une croix (échec), la
      ligne « Status: » avec son icône, les « Value: » en retrait et plus ternes. L'ascenseur est
      visible à droite ; le début (« Superview build ») est visible, la molette et l'ascenseur
      mènent jusqu'à « Log file » (dernière ligne entière) ; une longue raison d'échec ffmpeg
      passe à la ligne sans couper les lignes d'après ; la fenêtre ne coupe ni la liste ni les
      boutons. « Show as text » bascule vers le texte brut ([OK] / [FAIL]) : on y sélectionne
      (souris, Ctrl+A) et copie (Ctrl+C) dans un éditeur ; « Show as list » revient aux icônes.
      La liste ne défile pas au clavier (molette et ascenseur seulement) ; c'est la vue texte qui
      défile au clavier. Le test automatique mesure bien le texte (une ligne de 300 caractères
      doit être plus haute qu'une ligne courte, et la liste au moins aussi haute que ses lignes
      ensemble), mais pas le défilement lui-même ni le rendu des icônes : la fin de la liste a
      été vérifiée sous Xvfb le 2026-10-18 (une mise en page à base de layouts la coupait de
      quelques lignes). Les icônes sont dessinées, pas des emoji : le rendu logiciel de Slint ne
      dessine pas ✅/❌ (constaté à la main le 2026-10-18).

- [ ] **Lien d'installation dans le navigateur** (en suspend : filet de sécurité, la release embarque FFmpeg ; à passer
      seulement si ce code change) : sans ffmpeg dans le `PATH`, la popup « cannot find
      ffmpeg/ffprobe » s'ouvre ; un clic sur l'adresse ouvre le navigateur (`xdg-open` ou
      `explorer.exe`) sur la page de téléchargement. Retry après installation relance la sonde.

## Piège : la vraie fenêtre sous Xvfb

Pour lancer la fenêtre réelle sans écran, il faut **retirer `WAYLAND_DISPLAY`** :

```bash
env -u WAYLAND_DISPLAY xvfb-run -a -s "-screen 0 1280x800x24" target/debug/superview
```

Sans cela, winit vise Wayland (la session de la machine), ne trouve pas son affichage et
l'écran reste noir. Pour qu'aucun portail de la vraie session ne s'ouvre pendant l'essai,
ajouter `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` à l'application. Capture :
`ffmpeg -f x11grab -video_size 1280x800 -i $DISPLAY -frames:v 1 fenetre.png`. Pour lire la
propriété d'icône, `xprop` imprime une valeur vide pour `_NET_WM_ICON` même quand elle est
posée (mesuré) : la lire avec `XGetWindowProperty`.
