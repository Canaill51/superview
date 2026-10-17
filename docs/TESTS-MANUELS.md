# Tests manuels avant la pré-version du port Rust

> Journal interne (français). Ce que les tests automatiques ne peuvent pas voir : un vrai
> bureau, un vrai GPU, un vrai sélecteur de fichiers. À passer à la main, sur un binaire
> construit comme celui qui sera livré, avant d'annoncer la pré-version (phase 7). Cocher et
> dater ; noter la machine. Une case non cochée n'est pas un échec : c'est une inconnue.

Préparer un clip : `ffmpeg -f lavfi -i testsrc2=size=1440x1080:rate=30 -f lavfi -i sine -t 10 -c:v libx264 -c:a aac clip.mp4`

## Conversion

- [ ] **Seconde conversion** : convertir un clip, puis, sans fermer la fenêtre, en choisir un
      autre et convertir encore. Le bouton Start revient, la barre repart de zéro, la
      ligne « Hardware » est celle du nouveau fichier.
- [ ] **Annulation immédiate** : appuyer sur Cancel dès que Start est pris en compte. Aucun
      processus `ffmpeg` ne reste (`pgrep ffmpeg`), aucun fichier de sortie partiel, aucun
      dossier `superview-*` dans le dossier temporaire du système.
- [ ] **Source 10 bits** : `-pix_fmt yuv420p10le -c:v libx265` ; la conversion réussit et la
      sortie garde le 10 bits (`ffprobe`).
- [ ] **Plusieurs pistes audio** : un clip avec deux pistes ; les deux sont dans la sortie.
- [ ] **Fermeture pendant l'encodage** : croix de la fenêtre (et Quit) pendant une conversion :
      la question « Cancel and quit » s'affiche ; Yes arrête tout sans rien laisser derrière
      (mêmes vérifications que l'annulation) ; No laisse la conversion continuer.
- [ ] **SIGINT / SIGTERM sur la fenêtre** : lancer la conversion, puis `kill -INT <pid>` et
      `kill -TERM <pid>` (deux essais) : la fenêtre se ferme, la conversion se déroule, rien ne
      reste. Le trajet signal → fin de boucle n'est pas couvert par un test de fenêtre.

## Sélecteurs de fichiers (PR 6.4)

- [ ] **Windows** : « Choose input file » et « Choose output file » ouvrent la boîte native ;
      le filtre n'offre que les MP4 ; la sortie propose `output.mp4` ; un nom sans `.mp4`
      reçoit `.mp4` ; annuler ne change rien. Aucun repli n'existe sous Windows (DEC-43).
- [ ] **Linux avec portail** (GNOME, ou KDE avec `xdg-desktop-portal-kde`) : mêmes vérifications.
      Annuler vite (Échap tout de suite) ouvre peut-être le sélecteur de repli : c'est le prix
      du seuil de 300 ms, noter si cela gêne.
- [ ] **Linux sans portail, avec kdialog** (la machine KDE de l'utilisateur) : lancer avec
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
- [ ] **Icône de la fenêtre** : visible dans la barre de titre et dans la barre des tâches (Windows
      et Linux avec gestionnaire de fenêtres). Sous Xvfb sans gestionnaire, seule la propriété
      `_NET_WM_ICON` se mesure (1024x1024).

## Matériel

- [ ] **NVENC** (carte NVIDIA, pilote au moins 570) : « Hardware: » annonce NVENC ; la conversion
      l'utilise (`nvidia-smi` montre le processus) ; une source que NVENC refuse retombe sur le
      processeur.
- [ ] **VAAPI** (portable Intel) : idem avec VAAPI.
- [ ] **Test « device frame »** : sur le portable Intel VAAPI, passer
      `SUPERVIEW_REQUIRE_FFMPEG=1 cargo test -p superview-core --test real_ffmpeg -- device_frame`
      (`rust/crates/superview-core/tests/real_ffmpeg.rs`). Il saute en silence sans GPU : un
      vert sans matériel ne prouve rien.

## Divers

- [ ] **Lien d'installation dans le navigateur** : sans ffmpeg dans le `PATH`, la popup « cannot find
      ffmpeg/ffprobe » s'ouvre ; un clic sur l'adresse ouvre le navigateur (`xdg-open` ou
      `explorer.exe`) sur la page de téléchargement. Retry après installation relance la sonde.

## Piège : la vraie fenêtre sous Xvfb

Pour lancer la fenêtre réelle sans écran, il faut **retirer `WAYLAND_DISPLAY`** :

```bash
env -u WAYLAND_DISPLAY xvfb-run -a -s "-screen 0 1280x800x24" rust/target/debug/superview-app
```

Sans cela, winit vise Wayland (la session de la machine), ne trouve pas son affichage et
l'écran reste noir. Pour qu'aucun portail de la vraie session ne s'ouvre pendant l'essai,
ajouter `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` à l'application. Capture :
`ffmpeg -f x11grab -video_size 1280x800 -i $DISPLAY -frames:v 1 fenetre.png`. Pour lire la
propriété d'icône, `xprop` imprime une valeur vide pour `_NET_WM_ICON` même quand elle est
posée (mesuré) : la lire avec `XGetWindowProperty`.
