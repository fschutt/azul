// Go: nil where an Option is asked for, (T, bool) back from fields and
// downcasts, (T, error) for a Result, and the resume callback as a plain Go
// function through Bind3. Built and run by scripts/native_optionals_smoke.sh.
package main

import (
	"fmt"
	"os"

	azul "azul.rs/ui/go"
)

type model struct {
	Folder string
	Link   string
}

// The guide's file-dialog example, as written there (hello-world/go.md).
func onChooseFolder(m *model, _ *azul.CallbackInfo) azul.Update {
	azul.FileDialogOpenDirectory(azul.Str("Choose a folder"), nil, m, azul.Bind3(onPickFolder))
	return azul.Update_DoNothing
}

func onPickFolder(m *model, _ *azul.CallbackInfo, result any) azul.Update {
	picked, ok := azul.FileOpenResultDowncast(result)
	if !ok {
		return azul.Update_DoNothing
	}
	path, ok := picked.Path()
	if !ok {
		return azul.Update_DoNothing // cancelled
	}
	m.Folder = path.GetInner()
	return azul.Update_RefreshDom
}

func onOpenLink(m *model, _ *azul.CallbackInfo) azul.Update {
	if _, err := azul.UrlParse(azul.Str(m.Link)); err != nil {
		return azul.Update_DoNothing
	}
	return azul.Update_RefreshDom
}

func check(ok bool, what string) {
	if !ok {
		fmt.Println("FAIL: " + what)
		os.Exit(1)
	}
}

func main() {
	if err := azul.LoadLibrary(os.Getenv("AZ_LIB_DIR")); err != nil {
		panic(err)
	}

	// A field: nil in, false out; a value in, the value out.
	icons := azul.IconVariantsCreateDefault()
	_, ok := icons.Light()
	check(!ok, "a default IconVariants has no light icon")
	icons.SetLight(azul.Str("light.png"))
	light, ok := icons.Light()
	check(ok && light.Value() == "light.png", "Some in, Some out")
	icons.SetLight(nil)
	_, ok = icons.Light()
	check(!ok, "None in, None out")

	// A class payload: a Dom moved in, a Dom read back, nil clears it.
	view := azul.VirtualViewReturnCreateDefault()
	_, ok = view.Dom()
	check(!ok, "a default VirtualViewReturn has no Dom")
	view.SetDom(azul.DomCreateBody())
	_, ok = view.Dom()
	check(ok, "a Dom moved into an Option field reads back")
	view.SetDom(nil)
	_, ok = view.Dom()
	check(!ok, "nil clears the field")

	// A result: Ok is the value and a nil error, Err an *AzulError.
	_, err := azul.UrlParse(azul.Str("https://azul.rs/guide"))
	check(err == nil, "Url.parse Ok")
	_, err = azul.UrlParse(azul.Str("no scheme"))
	_, isAzul := err.(*azul.AzulError)
	check(err != nil && isAzul, "Url.parse Err")

	_ = onChooseFolder
	_ = onOpenLink
	fmt.Println("go native optionals: ok")
}
