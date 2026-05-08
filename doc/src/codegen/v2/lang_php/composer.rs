//! Composer manifest emission for the `azul` PHP package.
//!
//! The native shared library (`libazul.{so,dylib,dll}`) is **bundled
//! separately**: the Composer package only ships the PHP glue file
//! (`Azul.php`). Users must place the prebuilt native library somewhere
//! the dynamic loader can locate it (`LD_LIBRARY_PATH`, current working
//! directory, or `php.ini`'s configured search paths).
//!
//! The `ext-ffi` requirement is hard — the binding cannot function
//! without it. We pin to a permissive PHP range (`>=7.4`) to match the
//! version in which `FFI` shipped.

/// Generate the contents of `composer.json` as a String.
pub fn generate_composer_json() -> String {
    String::from(
        r#"{
    "name": "azul/azul-php",
    "description": "PHP FFI bindings for the Azul GUI framework",
    "type": "library",
    "license": ["MPL-2.0", "MIT", "Apache-2.0"],
    "homepage": "https://azul.rs",
    "require": {
        "php": ">=7.4",
        "ext-ffi": "*"
    },
    "autoload": {
        "psr-4": {
            "Azul\\": "src/"
        }
    },
    "authors": [
        {
            "name": "Azul Contributors",
            "homepage": "https://azul.rs"
        }
    ],
    "support": {
        "source": "https://github.com/maps4print/azul",
        "issues": "https://github.com/maps4print/azul/issues"
    }
}
"#,
    )
}
