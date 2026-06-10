#!/bin/bash
# Print "X Y W H" (frame geometry, global coords) of the first KWin window
# whose caption contains $1. Works on Plasma 5.x Wayland via KWin scripting;
# output is read back from the user journal (print() lands there).
set -u
match="${1:?usage: kwin-win-geom.sh <caption-substring>}"
js=$(mktemp /tmp/kwingeom-XXXXXX.js)
tag="AZWIN$$"
cat > "$js" <<EOF
var cs = workspace.clientList ? workspace.clientList() : workspace.windowList();
for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    if ((c.caption + "").indexOf("$match") !== -1) {
        var g = c.frameGeometry;
        print("$tag " + g.x + " " + g.y + " " + g.width + " " + g.height);
        break;
    }
}
EOF
id=$(qdbus org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" "geom$$")
# Plasma 5.27 exposes loaded scripts at object path /<id> (org.kde.kwin.Script)
qdbus org.kde.KWin "/${id}" org.kde.kwin.Script.run > /dev/null
qdbus org.kde.KWin "/${id}" org.kde.kwin.Script.stop > /dev/null
qdbus org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript "geom$$" > /dev/null
rm -f "$js"
journalctl --user -q --since "30 seconds ago" --no-pager 2>/dev/null \
    | grep -oE "$tag [0-9-]+ [0-9-]+ [0-9]+ [0-9]+" | tail -1 | cut -d' ' -f2-
