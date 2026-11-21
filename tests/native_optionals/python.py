# Python: None or the value where an Option is asked for, None or the value
# back, a ValueError for an Err. Run by scripts/native_optionals_smoke.sh
# against the python-extension build of libazul (imported as `azul`).
import sys

from azul import FileDialog, FileOpenResult, IconVariants, Update, Url


class DataModel:
    def __init__(self):
        self.folder = ""


# The guide's file-dialog example, as written there (hello-world/python.md).
def on_choose_folder(data, info):
    FileDialog.open_directory("Choose a folder", None, data, on_pick_folder)
    return Update.DoNothing


def on_pick_folder(data, info, result):
    picked = FileOpenResult.downcast(result)
    if picked is None or picked.path is None:
        return  # cancelled: nothing to do
    data.folder = picked.path.inner
    return Update.RefreshDom


def check(ok, what):
    if not ok:
        print("FAIL: " + what, file=sys.stderr)
        sys.exit(1)


# A field: None in, None out; a value in, the value out.
icons = IconVariants.default()
check(icons.light is None, "a default IconVariants has no light icon")
icons.light = "light.png"
check(icons.light == "light.png", "Some in, Some out")
icons.light = None
check(icons.light is None, "None in, None out")

# A result: the Ok value, or a raised ValueError.
check(Url.parse("https://azul.rs/guide") is not None, "Url.parse Ok")
try:
    Url.parse("no scheme")
    check(False, "Url.parse Err raises")
except ValueError:
    pass

# A downcast of something that is not a file dialog's answer is None.
check(FileOpenResult.downcast(DataModel()) is None, "downcast of another value is None")

print("python native optionals: ok")
