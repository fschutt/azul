#!/usr/bin/env python3
"""Tests of the pure parts of scripts/ci/jni_signature_check.py (python3 -I scripts/ci/test_jni_signature_check.py)."""
import sys
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import jni_signature_check as j  # noqa: E402

RUST = '''
const RESOLVER_QUERY: &str =
    "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;";
const ACTION_SEND: &str = "android.intent.action.SEND";
fn demo(env: &mut JNIEnv) {
    let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
    env.call_method(&resolver, "query", RESOLVER_QUERY, &[]);
    env.call_method(&descriptor, "getFd", "()I", &[]).and_then(|v| v.i());
    env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[]);
    env.get_static_field(DOCUMENT_CLASS, "FLAG_SUPPORTS_RENAME", "I");
    env.call_method(&x, "typo", "(Ljava/lang/String)V", &[]);
}
'''

JAVAP = '''Compiled from "ContentResolver.java"
public abstract class android.content.ContentResolver {
  public final android.database.Cursor query(android.net.Uri, java.lang.String[], java.lang.String, java.lang.String[], java.lang.String);
    descriptor: (Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;
  public static final java.lang.String SCHEME_CONTENT;
    descriptor: Ljava/lang/String;
}
public class android.content.Intent implements android.os.Parcelable {
  public android.content.Intent(java.lang.String);
    descriptor: (Ljava/lang/String;)V
}
public class android.os.ParcelFileDescriptor {
  public int getFd();
    descriptor: ()I
}
public class android.net.Uri {
  public static android.net.Uri parse(java.lang.String);
    descriptor: (Ljava/lang/String;)Landroid/net/Uri;
}
public class android.provider.DocumentsContract$Document {
  public static final int FLAG_SUPPORTS_RENAME;
    descriptor: I
}
'''


class ReadingTheCalls(unittest.TestCase):
    def test_the_consts_of_a_file_are_read_even_when_their_value_is_on_the_next_line(self):
        consts = j.consts_of(RUST)
        self.assertTrue(consts["RESOLVER_QUERY"].startswith("(Landroid/net/Uri;"))
        self.assertEqual(consts["ACTION_SEND"], "android.intent.action.SEND")

    def test_a_call_is_a_name_and_a_descriptor_either_of_them_a_const(self):
        calls = set(j.calls_of(RUST))
        self.assertIn(("query", j.consts_of(RUST)["RESOLVER_QUERY"]), calls)
        self.assertIn(("getFd", "()I"), calls)
        self.assertIn(("parse", "(Ljava/lang/String;)Landroid/net/Uri;"), calls)
        self.assertIn(("<init>", "(Ljava/lang/String;)V"), calls)
        self.assertNotIn(("android.intent.action.SEND", "x"), calls)

    def test_a_field_read_is_a_name_and_a_descriptor(self):
        self.assertEqual(j.fields_of('env.get_static_field("a/B", "FLAG", "I")'), [("FLAG", "I")])

    def test_the_classes_a_file_names_come_with_the_supertypes_a_call_may_be_made_on(self):
        classes = j.classes_of(RUST)
        self.assertIn("android/content/Intent", classes)
        self.assertIn("android/net/Uri", classes)
        self.assertIn("android/content/Context", classes)


class Constants(unittest.TestCase):
    def test_the_sdk_constants_a_file_reads_by_name_are_found_but_not_the_values_of_its_own_consts(self):
        source = 'const A: &str = "EXTRA_THING_X";\nstatic_int(env, DOCUMENT_CLASS, "FLAG_SUPPORTS_RENAME")?; for f in ["COLUMN_FLAGS", "MIME_TYPE_DIR"] {}'
        self.assertEqual(j.constants_of(source), ["COLUMN_FLAGS", "FLAG_SUPPORTS_RENAME", "MIME_TYPE_DIR"])

    def test_a_constant_that_no_class_has_is_reported(self):
        methods, fields = j.parse_javap(JAVAP)
        self.assertEqual(j.missing(set(), set(), methods, fields, ["FLAG_SUPPORTS_RENAME", "FLAG_NOPE"]), [("FLAG_NOPE", "constant")])


class ReadingJavap(unittest.TestCase):
    def test_methods_constructors_and_fields_are_read_with_their_descriptors(self):
        methods, fields = j.parse_javap(JAVAP)
        self.assertIn(("query", "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;"), methods)
        self.assertIn(("<init>", "(Ljava/lang/String;)V"), methods)
        self.assertIn(("getFd", "()I"), methods)
        self.assertIn(("SCHEME_CONTENT", "Ljava/lang/String;"), fields)
        self.assertIn(("FLAG_SUPPORTS_RENAME", "I"), fields)


class Finding(unittest.TestCase):
    def test_a_typo_in_a_descriptor_is_the_only_thing_reported(self):
        methods, fields = j.parse_javap(JAVAP)
        bad = j.missing(set(j.calls_of(RUST)), set(j.fields_of(RUST)), methods, fields)
        # the unparsable `typo` descriptor is not even a call (a descriptor must be well formed); the field read is found
        self.assertEqual(bad, [])
        wrong = 'env.call_method(&x, "getFd", "()J", &[]); env.call_method(&x, "getFD", "()I", &[]);'
        bad = j.missing(set(j.calls_of(wrong)), set(), methods, fields)
        self.assertEqual(bad, [("getFD", "()I"), ("getFd", "()J")])


if __name__ == "__main__":
    unittest.main()
