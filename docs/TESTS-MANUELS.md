# Tests manuels avant la pré-version du port Rust

> Journal interne (français). Ce que les tests automatiques ne peuvent pas voir : un vrai
> bureau, un vrai GPU, un vrai sélecteur de fichiers. À passer à la main, sur un binaire
> construit comme celui qui sera livré, avant d'annoncer la pré-version (phase 7). Cocher et
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
- [ ] **Icône de la fenêtre** : visible dans la barre de titre et dans la barre des tâches (Windows
      et Linux avec gestionnaire de fenêtres). Sous Xvfb sans gestionnaire, seule la propriété
      `_NET_WM_ICON` se mesure (1024x1024).

## Matériel

- [x] **NVENC** (carte NVIDIA, pilote au moins 570) : « Hardware: » annonce NVENC ; la conversion
      l'utilise (`nvidia-smi` montre le processus) ; une source que NVENC refuse retombe sur le
      processeur.
- [ ] **VAAPI** (portable Intel) : idem avec VAAPI.
- [ ] **GPU Intel seul** (DEC-48 ; Nvidia masquée ou absente, pilote VAAPI sans firmware HuC qui
      répond `supported modes: CQP`) : le Diagnostic dit `✅ h264_vaapi: works, constant quality
      only` ; « Hardware: » finit par « (constant quality) » ; la conversion produit un MP4 16:9
      lisible (`ffprobe` : codec h264, pas de débit demandé respecté, taille qui suit le contenu) ;
      le journal garde le refus en débit et dit que le débit demandé n'est pas utilisé.
- [ ] **Test « device frame »** : sur le portable Intel VAAPI, passer
      `SUPERVIEW_REQUIRE_FFMPEG=1 cargo test -p superview-core --test real_ffmpeg -- device_frame`
      (`rust/crates/superview-core/tests/real_ffmpeg.rs`). Il saute en silence sans GPU : un
      vert sans matériel ne prouve rien.

## Divers

- [ ] **Diagnostic : lire tout le rapport** : le bouton Diagnostic ouvre le rapport en liste : une
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

- [ ] **Lien d'installation dans le navigateur** : sans ffmpeg dans le `PATH`, la popup « cannot find
      ffmpeg/ffprobe » s'ouvre ; un clic sur l'adresse ouvre le navigateur (`xdg-open` ou
      `explorer.exe`) sur la page de téléchargement. Retry après installation relance la sonde.

## Piège : la vraie fenêtre sous Xvfb

Pour lancer la fenêtre réelle sans écran, il faut **retirer `WAYLAND_DISPLAY`** :

```bash
env -u WAYLAND_DISPLAY xvfb-run -a -s "-screen 0 1280x800x24" rust/target/debug/superview
```

Sans cela, winit vise Wayland (la session de la machine), ne trouve pas son affichage et
l'écran reste noir. Pour qu'aucun portail de la vraie session ne s'ouvre pendant l'essai,
ajouter `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` à l'application. Capture :
`ffmpeg -f x11grab -video_size 1280x800 -i $DISPLAY -frames:v 1 fenetre.png`. Pour lire la
propriété d'icône, `xprop` imprime une valeur vide pour `_NET_WM_ICON` même quand elle est
posée (mesuré) : la lire avec `XGetWindowProperty`.
