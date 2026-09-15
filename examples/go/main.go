package main

import (
	"fmt"

	azul "github.com/azul/azul-go"
)



type counterModel struct {
	Counter int
}

func onClick(data *azul.RefAny, _ *azul.CallbackInfo) azul.AzUpdate {
	v, ok := azul.RefAnyGet(data)
	if !ok {
		return azul.AzUpdate_DoNothing
	}
	model, ok := v.(*counterModel)
	if !ok {
		return azul.AzUpdate_DoNothing
	}
	model.Counter++
	return azul.AzUpdate_RefreshDom
}

func layout(data *azul.RefAny, _ *azul.LayoutCallbackInfo) *azul.Dom {
	body := azul.NewDomCreateBody()

	v, ok := azul.RefAnyGet(data)
	if !ok {
		return body
	}
	model, ok := v.(*counterModel)
	if !ok {
		return body
	}

	label := azul.NewDomCreatePWithText(azul.Str(fmt.Sprintf("%d", model.Counter)))

	button := azul.NewButtonCreate(azul.Str("Increase counter"))
	button.OnClick(data, onClick)

	body.SetCss(azul.Str("p { font-size: 32px; margin: 0; }"))
	body.AddChild(label.Raw())
	body.AddChild(button.Dom().Raw())
	return body
}

func main() {
	app := azul.NewAppWithData(&counterModel{Counter: 5}, nil)
	app.RunWindow(azul.NewWindowCreateOptions(layout))
}
