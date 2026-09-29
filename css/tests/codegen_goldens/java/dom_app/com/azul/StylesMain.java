package com.azul;

public final class StylesMain {
    public static void main(String[] args) {
        AzCssPropertyWithConditionsVec.ByValue renderUi = AzulStyles.renderUi();
        System.out.println("renderUi: " + renderUi.len + " properties");
    }
}
