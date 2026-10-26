# Trouve ffmpeg et ffprobe là où les tests les trouveront (à sourcer).
#   outil_trouve ffmpeg && echo oui
# Même ordre que `ToolResolver` (crates/superview-core/src/tools.rs) pour ce qui compte ici :
# `SUPERVIEW_FFMPEG_DIR` s'il est absolu (un dossier relatif est ignoré, comme là-bas), puis le
# PATH. Sous Windows, seul `<outil>.exe` compte dans le dossier (DEC-45). Ne chercher que dans le
# PATH refusait un poste dont ffmpeg n'est que dans ce dossier, alors que la suite y passe.
# Aucun programme externe (`$OSTYPE`, pas `uname`) : scripts/checks/outils.sh vide le PATH.
outil_trouve() {
  local dir="${SUPERVIEW_FFMPEG_DIR:-}" nom="$1"
  case "${OSTYPE:-}" in msys* | cygwin*) nom="$1.exe" ;; esac
  case "$dir" in
    /* | [A-Za-z]:[\\/]*) [ -f "$dir/$nom" ] && [ -x "$dir/$nom" ] && return 0 ;;
  esac
  command -v "$1" >/dev/null 2>&1
}
