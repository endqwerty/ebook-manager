"""Render the menu entry with the executable's final installation path."""

from pathlib import Path
import sys

prefix = Path(sys.argv[1])
if not prefix.is_absolute():
    sys.exit("PREFIX must be an absolute installation path")
binary = str(prefix / "bin" / "ebook-manager")
if "=" in binary or any(ord(c) < 32 for c in binary):
    sys.exit("The desktop launcher path cannot contain '=' or control characters")

# Exec quoting is decoded after desktop-entry string escaping.
quoted = "".join("\\" + c if c in '`"$\\' else c for c in binary)
command = '"' + quoted.replace("\\", "\\\\").replace("%", "%%") + '"'
try_exec = binary.replace("\\", "\\\\")
template = Path(__file__).with_name("io.github.endqwerty.EbookManager.desktop")
entry = template.read_text().replace(
    "Exec=ebook-manager\n", f"Exec={command}\nTryExec={try_exec}\n"
)
print(entry, end="")
