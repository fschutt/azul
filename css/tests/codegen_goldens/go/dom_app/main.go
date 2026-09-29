package main

import (
	"fmt"

	azul "azul.rs/ui/go"

	"azul-styles/styles"
)

func main() {
	if err := azul.LoadLibrary(""); err != nil {
		panic(err)
	}
	renderUi := styles.RenderUi()
	fmt.Printf("RenderUi: %d properties\n", renderUi.Len)
}
