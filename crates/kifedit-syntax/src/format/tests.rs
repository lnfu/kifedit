use expect_test::{Expect, expect};

use super::*;
use crate::parser::parse;

fn format_str(src: &str, options: &FormatOptions) -> String {
    format_file(&parse(src).syntax(), options)
}

fn check_with(options: FormatOptions, src: &str, expect: Expect) {
    let out = format_str(src, &options);
    expect.assert_eq(&out);
    assert_eq!(
        format_str(&out, &options),
        out,
        "The formatter output changes when you format it again."
    );
}

fn check(src: &str, expect: Expect) {
    check_with(FormatOptions::default(), src, expect);
}

#[test]
fn keeps_line_break_before_documentation_string() {
    check(
        "(documentation ShutterTypeAttribute ChineseTraditionalLanguage\n  \"相機快門型式的屬性類別。\")\n",
        expect![[r#"
            (documentation ShutterTypeAttribute ChineseTraditionalLanguage
              "相機快門型式的屬性類別。")
        "#]],
    );
}

#[test]
fn keeps_short_documentation_on_one_line() {
    check(
        "(documentation Camera EnglishLanguage \"A device for taking pictures.\")\n",
        expect![[r#"
            (documentation Camera EnglishLanguage "A device for taking pictures.")
        "#]],
    );
}

#[test]
fn moves_long_string_to_next_line() {
    check(
        "(documentation Entity EnglishLanguage \"The universal class of individuals. This is the root node of the ontology.\")\n",
        expect![[r#"
            (documentation Entity EnglishLanguage
              "The universal class of individuals. This is the root node of the ontology.")
        "#]],
    );
}

#[test]
fn breaks_each_argument_when_prefix_does_not_fit() {
    let narrow = FormatOptions {
        line_width: 20,
        ..FormatOptions::default()
    };
    check_with(
        narrow,
        "(rosParameter HeadLuxonisCameraNode \"is_gripper\" \"false\")\n",
        expect![[r#"
            (rosParameter
              HeadLuxonisCameraNode
              "is_gripper"
              "false")
        "#]],
    );
}

#[test]
fn reindents_rule_with_line_start_lists() {
    // Biography.kif:428，行尾有一個多餘的空格
    check(
        "(=>\n(attribute ?PERSON Economist)\n(hasExpertise ?PERSON Economics)) \n",
        expect![[r#"
            (=>
              (attribute ?PERSON Economist)
              (hasExpertise ?PERSON Economics))
        "#]],
    );
}

#[test]
fn always_breaks_short_rule() {
    check(
        "(=> (networkUplink ?A ?B) (connected ?A ?B))\n",
        expect![[r#"
        (=>
          (networkUplink ?A ?B)
          (connected ?A ?B))
    "#]],
    );
}

#[test]
fn nested_logic_and_quantifier() {
    check(
        "(=> (and (instance ?X Camera) (exists (?Y) (part ?Y ?X))) (instance ?X Device))\n",
        expect![[r#"
            (=>
              (and
                (instance ?X Camera)
                (exists (?Y)
                  (part ?Y ?X)))
              (instance ?X Device))
        "#]],
    );
}

#[test]
fn keeps_comments_and_limits_blank_lines() {
    check(
        ";;; header\n\n\n\n(instance A B) ; trailing\n;; before rule\n(=>\n  ; inside\n  (a ?X)\n  (b ?X) ; last\n  )\n",
        expect![[r#"
            ;;; header


            (instance A B) ; trailing
            ;; before rule
            (=>
              ; inside
              (a ?X)
              (b ?X) ; last
            )
        "#]],
    );
}

#[test]
fn multi_line_string_is_unchanged() {
    check(
        "(documentation GripperCamera ChineseTraditionalLanguage\n  \"第一行。\n目前的使用方式：只當作相機。\n- 第三行\")\n",
        expect![[r#"
            (documentation GripperCamera ChineseTraditionalLanguage
              "第一行。
            目前的使用方式：只當作相機。
            - 第三行")
        "#]],
    );
}

#[test]
fn error_form_is_verbatim() {
    check(
        "(a (b\n(instance   A    B )\n",
        expect![[r#"
        (a (b
        (instance A B)
    "#]],
    );
}

#[test]
fn normalizes_spaces_and_file_edges() {
    check(
        "\n\n  (instance   A    B )\n\n\n",
        expect![[r#"
        (instance A B)
    "#]],
    );
}

#[test]
fn nested_function_term_stays_on_one_line() {
    check(
        "(monetaryValue MyWifiAP (MeasureFn 6370 TaiwanDollar))\n",
        expect![[r#"
            (monetaryValue MyWifiAP (MeasureFn 6370 TaiwanDollar))
        "#]],
    );
}

#[test]
fn keeps_crlf_line_endings() {
    let out = format_str("(a  b)\r\n(=> (c) (d))\r\n", &FormatOptions::default());
    assert_eq!(out, "(a b)\r\n(=>\r\n  (c)\r\n  (d))\r\n");
}
