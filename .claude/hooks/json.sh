# Lit un champ du JSON que Claude Code passe aux hooks sur l'entrée standard (à sourcer).
#   printf '%s' "$entree" | champ_json tool_input.command
# Perl et JSON::PP, présents dans Git Bash sous Windows et sur tout Linux. Pas python3 : sous
# Windows c'est le raccourci du Microsoft Store, qui échoue, et les hooks lisaient alors un champ
# vide sans rien dire. Code de retour non nul si l'entrée ne se lit pas : à l'appelant de ne pas
# le prendre pour un champ vide. Un booléen s'écrit `true` ou `false`, un champ absent ``.
champ_json() {
  perl -MJSON::PP -e '
    local $/;
    my $v = eval { decode_json(<STDIN>) } or exit 3;
    for my $k (split /\./, $ARGV[0]) { $v = ref $v eq "HASH" ? $v->{$k} : undef }
    $v = "" unless defined $v;
    $v = $v ? "true" : "false" if JSON::PP::is_bool($v);
    binmode STDOUT, ":encoding(UTF-8)";
    print $v;
  ' "$1"
}
