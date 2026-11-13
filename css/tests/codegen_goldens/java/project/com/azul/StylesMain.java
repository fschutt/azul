package com.azul;

public final class StylesMain {
    public static void main(String[] args) {
        AzCssPropertyWithConditionsVec.ByValue styleBtn = AzulStyles.styleBtn();
        System.out.println("styleBtn: " + styleBtn.len + " properties");
    }
}
