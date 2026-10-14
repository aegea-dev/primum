use primum::engine::eval::{Vars, render, truth, value};
use serde_json::{Map, Value, json};

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn expressions_see_the_ticket_input_prev_and_output() {
    let ticket = obj(json!({"id": "12", "title": "Çay"}));
    let (project, args) = (obj(json!({"bölge": "eu"})), obj(json!({"deneme": 3})));
    let input = json!({"text": "ışık"});
    let prev = json!({"json": {"data": {"name": "Ürün"}}});
    let output = json!({"status": 200});
    let v = Vars {
        ticket: &ticket,
        project: &project,
        args: &args,
        input: &input,
        prev: &prev,
        output: Some(&output),
    };
    assert!(truth("output.status == 200 && has(prev.json.data)", &v).unwrap());
    assert!(!truth("output.status >= 300", &v).unwrap());
    assert_eq!(
        value("prev.json.data", &v).unwrap(),
        json!({"name": "Ürün"})
    );
    assert_eq!(
        value("ticket.id + '/' + ticket.title", &v).unwrap(),
        json!("12/Çay")
    );
    assert_eq!(value("project['bölge']", &v).unwrap(), json!("eu"));
    assert_eq!(value("args.deneme + 1", &v).unwrap(), json!(4));
    assert_eq!(value("input.text", &v).unwrap(), json!("ışık"));
    assert!(value("prev.json.yok", &v).unwrap_err().contains("yok"));
    assert!(
        truth("output.status", &v)
            .unwrap_err()
            .contains("true or false")
    );
}

#[test]
fn templates_keep_a_lone_values_type_and_interpolate_text() {
    let ticket = obj(json!({"id": "12", "title": "Çay"}));
    let empty = Map::new();
    let prev = json!({"name": "Ürün", "n": 3});
    let v = Vars {
        ticket: &ticket,
        project: &empty,
        args: &empty,
        input: &Value::Null,
        prev: &prev,
        output: None,
    };
    let with = json!({
        "url": "https://api.example.com/items/${ticket.id}?n=${prev.n}",
        "whole": "${prev}",
        "count": "${prev.n}",
        "auth": "Bearer ${env.API_KEY}",
        "list": ["${ticket.title}", 7],
        "plain": "no templates, ğüşı"
    });
    assert_eq!(
        render(&with, &v).unwrap(),
        json!({
            "url": "https://api.example.com/items/12?n=3",
            "whole": {"name": "Ürün", "n": 3},
            "count": 3,
            "auth": "Bearer ${env.API_KEY}",
            "list": ["Çay", 7],
            "plain": "no templates, ğüşı"
        })
    );
    assert!(
        render(&json!("${prev.yok}"), &v)
            .unwrap_err()
            .contains("yok")
    );
}
