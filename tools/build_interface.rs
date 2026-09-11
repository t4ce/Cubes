//! Build-time editable JSON -> typed menu definitions; no target JSON dependency.
use serde_json::Value;
use std::{fs, path::Path};
pub fn generate(root: &Path, output: &Path) {
    let mut generated = String::from("pub const EXAMPLES: &[Definition] = &[\n");
    for name in ["confirm", "info", "slider"] {
        let path = root.join(format!("{name}.json"));
        println!("cargo:rerun-if-changed={}", path.display());
        let value: Value = serde_json::from_slice(&fs::read(&path).expect("interface JSON"))
            .expect("valid interface JSON");
        assert_eq!(
            value["format"],
            "subcubes-interface",
            "{} format",
            path.display()
        );
        assert_eq!(value["version"], 2);
        assert_eq!(value["preset"], "custom");
        let choice = |field: &Value, choices: &[&str]| {
            choices
                .iter()
                .position(|v| field.as_str() == Some(v))
                .expect("supported interface option")
        };
        let theme = choice(&value["themeId"], &["fern", "glacier", "ember"]);
        let tier = [1, 2, 3, 4, 6, 8, 12][choice(
            &value["tierId"],
            &["c1", "c2", "c3", "c4", "c6", "c8", "c12"],
        )];
        let menu = &value["menu"];
        let title = menu["title"].as_str().expect("menu title");
        let text = menu["text"].as_str().expect("menu text");
        assert!(title.chars().count() <= 30 && text.chars().count() <= 2048);
        let count = menu["buttonCount"].as_u64().expect("buttonCount");
        assert!((1..=3).contains(&count));
        let buttons = menu["buttons"].as_array().expect("buttons");
        assert!(buttons.len() >= count as usize && buttons.len() <= 3);
        generated.push_str(&format!("Definition {{ name: {name:?}, title: {title:?}, text: {text:?}, theme: {theme}, tier: {tier}, buttons: &[\n"));
        for button in buttons.iter().take(count as usize) {
            let mode = choice(&button["mode"], &["text", "icon", "both"]);
            let icon = choice(
                &button["icon"],
                &[
                    "close", "check", "plus", "minus", "left", "right", "heart", "menu", "play",
                    "gear",
                ],
            );
            let text = button["text"].as_str().expect("button text");
            assert!(text.chars().count() <= 14);
            generated.push_str(&format!(
                "Button {{ mode: {mode}, icon: {icon}, text: {text:?} }},\n"
            ));
        }
        let controls: Vec<bool> = ["slider", "checkbox", "toggle", "counter"]
            .iter()
            .map(|key| menu["controls"][key].as_bool().expect("control boolean"))
            .collect();
        let state = &value["state"];
        let progress = state["progress"].as_i64().expect("slider progress");
        let count = state["count"].as_i64().expect("counter count");
        assert!((0..=100).contains(&progress) && (-999..=999).contains(&count));
        let enabled = state["enabled"].as_bool().expect("enabled");
        let checked = state["checked"].as_bool().expect("checked");
        generated.push_str(&format!("], controls: {controls:?}, initial: State {{ progress: {progress}, count: {count}, enabled: {enabled}, checked: {checked} }} }},\n"));
    }
    generated.push_str("];\n");
    fs::write(output, generated).expect("write compiled interface definitions");
}
