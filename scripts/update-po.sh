#!/bin/sh
# Regenerates po/rutile.pot from the sources, then merges it into every
# po/*.po (new strings appear untranslated, changed ones as fuzzy).
#
# Maintainer-side only — the build itself reads po/*.po with no external
# tool (build.rs). Needs GNU gettext >= 0.24, the first xgettext that reads
# Rust (Ubuntu 24.04 ships 0.21; Arch/Fedora are recent enough).
#
# Start a new language with:
#   msginit --no-translator -i po/rutile.pot -l <code> -o po/<code>.po
set -eu
cd "$(dirname "$0")/.."

# The header (no --omit-header) declares UTF-8: without it xgettext
# writes ASCII and silently drops every non-ASCII character of a msgid.
# shellcheck disable=SC2046 # one argument per source file is the point
LC_ALL=C.UTF-8 xgettext \
    --language=Rust --from-code=UTF-8 \
    --keyword= --keyword=tr --keyword=trn:1,2 --keyword=tr_args --keyword=n_ \
    --add-comments=TRANSLATORS \
    --package-name=Rutile \
    --msgid-bugs-address=https://github.com/yatoub/Rutile/issues \
    --sort-by-file \
    --output=po/rutile.pot \
    $(git ls-files 'src/*.rs')

for po in po/*.po; do
    [ -e "$po" ] || continue
    msgmerge --quiet --update --backup=none --previous "$po" po/rutile.pot
    msgfmt --check --statistics --output-file=/dev/null "$po"
done
