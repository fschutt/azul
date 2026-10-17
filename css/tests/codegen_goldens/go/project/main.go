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
	styleBtn := styles.StyleBtn()
	fmt.Printf("StyleBtn: %d properties\n", styleBtn.Len)
}
