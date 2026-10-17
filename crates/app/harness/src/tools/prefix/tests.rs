use serde_json::json;

use super::*;

#[test]
fn every_prefix_of_a_post_reads_as_what_has_arrived() {
    let input = r#"{"channel":"general","reply_to":12,"text":"Hi \"you\" \u00e9!"}"#;
    let cases = [
        ("", json!(null), json!({})),
        ("{", json!({}), json!({})),
        (r#"{"chan"#, json!({}), json!({})),
        (r#"{"channel":"gen"#, json!({ "channel": "gen" }), json!({})),
        (
            r#"{"channel":"general""#,
            json!({ "channel": "general" }),
            json!({ "channel": "general" }),
        ),
        (
            r#"{"channel":"general","reply_to":1"#,
            json!({ "channel": "general" }),
            json!({ "channel": "general" }),
        ),
        (
            r#"{"channel":"general","reply_to":12,"#,
            json!({ "channel": "general", "reply_to": 12 }),
            json!({ "channel": "general", "reply_to": 12 }),
        ),
        (
            r#"{"channel":"general","reply_to":12,"text":"Hi \"yo"#,
            json!({ "channel": "general", "reply_to": 12, "text": "Hi \"yo" }),
            json!({ "channel": "general", "reply_to": 12 }),
        ),
        (
            r#"{"channel":"general","reply_to":12,"text":"Hi \"you\" \u00"#,
            json!({ "channel": "general", "reply_to": 12, "text": "Hi \"you\" " }),
            json!({ "channel": "general", "reply_to": 12 }),
        ),
        (
            r#"{"channel":"general","reply_to":12,"text":"Hi \"you\" \"#,
            json!({ "channel": "general", "reply_to": 12, "text": "Hi \"you\" " }),
            json!({ "channel": "general", "reply_to": 12 }),
        ),
        (
            input,
            json!({ "channel": "general", "reply_to": 12, "text": "Hi \"you\" é!" }),
            json!({ "channel": "general", "reply_to": 12, "text": "Hi \"you\" é!" }),
        ),
    ];
    for (text, arrived, whole) in cases {
        let read = prefix(text);
        assert_eq!(read.arrived, arrived, "{text}");
        assert_eq!(Value::Object(read.whole), whole, "{text}");
    }
}

#[test]
fn a_nested_value_still_open_is_not_whole() {
    let read = prefix(r#"{"a":[1,{"b":"x"#);
    assert_eq!(read.arrived, json!({ "a": [1, { "b": "x" }] }));
    assert!(read.settled("a").is_none());
    let read = prefix(r#"{"a":[1,{"b":"x"}],"c":tr"#);
    assert_eq!(read.settled("a"), Some(&json!([1, { "b": "x" }])));
    assert!(read.arriving("c").is_none());
    let read = prefix(r#"{"a":true }"#);
    assert_eq!(read.settled("a"), Some(&json!(true)));
}

#[test]
fn every_char_boundary_of_a_text_parses_without_panic() {
    let input = r#"{"channel":"보고서","text":"line one\nline two ✓ \\ done"}"#;
    for (at, _) in input.char_indices() {
        let read = prefix(&input[..at]);
        let text = read
            .arriving("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(
            "line one\nline two ✓ \\ done".starts_with(text),
            "{at}: {text:?}"
        );
    }
}
