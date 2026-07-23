use std::collections::HashSet;

pub struct DeconjRule {
    pub strip_suffix: String,
    pub append_suffix: String,
    pub tag: &'static str,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DeconjugatedForm {
    pub text: String,
    pub rule_chain: Vec<&'static str>,
}

struct GodanRow {
    dict_ending: char,
    a: char, // negative/passive/causative stem
    i: char, // masu stem
    e: char, // conditional/potential stem
    o: char, // volitional stem
    te: &'static str,
    ta: &'static str,
}

const GODAN_ROWS: &[GodanRow] = &[
    GodanRow { dict_ending: 'う', a: 'わ', i: 'い', e: 'え', o: 'お', te: "って", ta: "った" },
    GodanRow { dict_ending: 'く', a: 'か', i: 'き', e: 'け', o: 'こ', te: "いて", ta: "いた" },
    GodanRow { dict_ending: 'ぐ', a: 'が', i: 'ぎ', e: 'げ', o: 'ご', te: "いで", ta: "いだ" },
    GodanRow { dict_ending: 'す', a: 'さ', i: 'し', e: 'せ', o: 'そ', te: "して", ta: "した" },
    GodanRow { dict_ending: 'つ', a: 'た', i: 'ち', e: 'て', o: 'と', te: "って", ta: "った" },
    GodanRow { dict_ending: 'ぬ', a: 'な', i: 'に', e: 'ね', o: 'の', te: "んで", ta: "んだ" },
    GodanRow { dict_ending: 'ぶ', a: 'ば', i: 'び', e: 'べ', o: 'ぼ', te: "んで", ta: "んだ" },
    GodanRow { dict_ending: 'む', a: 'ま', i: 'み', e: 'め', o: 'も', te: "んで", ta: "んだ" },
    GodanRow { dict_ending: 'る', a: 'ら', i: 'り', e: 'れ', o: 'ろ', te: "って", ta: "った" },
];

pub fn build_deconjugation_rules() -> Vec<DeconjRule> {
    let mut rules = Vec::new();

    // ── Godan verbs (row-specific stem changes) ──
    for row in GODAN_ROWS {
        let (a, i, e, o) = (row.a, row.i, row.e, row.o);
        let dict = row.dict_ending.to_string();

        let mut forms: Vec<(String, &'static str)> = vec![
            (format!("{a}ない"), "negative"),
            (format!("{a}なかった"), "past negative"),
            (format!("{a}れる"), "passive"),
            (format!("{a}せる"), "causative"),
            (format!("{a}なければならない"), "must"),
            (format!("{a}なければならなかった"), "had to"),
            (format!("{a}なければなりません"), "must (polite)"),
            (format!("{a}なくてはいけない"), "must (alt)"),
            (format!("{i}ます"), "polite"),
            (format!("{i}ません"), "polite negative"),
            (format!("{i}ました"), "polite past"),
            (format!("{i}ませんでした"), "polite past negative"),
            (format!("{i}ましょう"), "polite volitional"),
            (format!("{i}たい"), "want to"),
            (format!("{i}ながら"), "while doing"),
            (format!("{i}なさい"), "imperative (polite)"),
            (format!("{e}ば"), "conditional"),
            (format!("{e}る"), "potential"),
            (row.te.to_string(), "te-form"),
            (row.ta.to_string(), "past"),
            (format!("{o}う"), "volitional"),
        ];

        let te_iru_forms: Vec<(String, &'static str)> = vec![
            (format!("{}いる", row.te), "progressive"),
            (format!("{}いた", row.te), "progressive past"),
            (format!("{}います", row.te), "progressive polite"),
            (format!("{}いました", row.te), "progressive polite past"),
            (format!("{}いません", row.te), "progressive polite negative"),
        ];
        forms.extend(te_iru_forms);

        for (suffix, tag) in forms {
            rules.push(DeconjRule { strip_suffix: suffix, append_suffix: dict.clone(), tag });
        }
    }

    // ── Ichidan verbs (uniform stem, no sound changes) ──
    let ichidan_forms: &[(&str, &str)] = &[
            ("ない", "negative"), ("なかった", "past negative"),
            ("ます", "polite"), ("ません", "polite negative"),
            ("ました", "polite past"), ("ませんでした", "polite past negative"),
            ("ましょう", "polite volitional"), ("たい", "want to"),
            ("ながら", "while doing"), ("なさい", "imperative (polite)"),
            ("て", "te-form"), ("た", "past"), ("れば", "conditional"),
            ("られる", "potential/passive"), ("させる", "causative"),
            ("よう", "volitional"),
            ("なければならない", "must"), ("なければならなかった", "had to"),
            ("なければなりません", "must (polite)"),
            ("ている", "progressive"), ("ていた", "progressive past"),
            ("ています", "progressive polite"), ("ていました", "progressive polite past"),
            ("ていません", "progressive polite negative"),
        ];
    for (suffix, tag) in ichidan_forms {
        rules.push(DeconjRule { strip_suffix: suffix.to_string(), append_suffix: "る".to_string(), tag });
    }

    // ── する (irregular) ──
    let suru_forms: &[(&str, &str)] = &[
            ("しない", "negative"), ("しなかった", "past negative"),
            ("します", "polite"), ("しません", "polite negative"),
            ("しました", "polite past"), ("しませんでした", "polite past negative"),
            ("しましょう", "polite volitional"), ("したい", "want to"),
            ("して", "te-form"), ("した", "past"), ("すれば", "conditional"),
            ("される", "passive"), ("させる", "causative"), ("しよう", "volitional"),
            ("できる", "potential"),
            ("しなければならない", "must"), ("しなければならなかった", "had to"),
            ("している", "progressive"), ("していた", "progressive past"),
            ("しています", "progressive polite"), ("していました", "progressive polite past"),
            ("していません", "progressive polite negative"),
        ];
    for (suffix, tag) in suru_forms {
        // these strip the whole する-stem's conjugated form and replace with literal "する"
        rules.push(DeconjRule { strip_suffix: suffix.to_string(), append_suffix: "する".to_string(), tag });
    }

    // ── 来る (irregular, kanji stem stays but reading changes — handled as literal) ──
    let kuru_forms: &[(&str, &str)] = &[
            ("来ない", "negative"), ("来なかった", "past negative"),
            ("来ます", "polite"), ("来ません", "polite negative"),
            ("来ました", "polite past"), ("来て", "te-form"), ("来た", "past"),
            ("来られる", "potential/passive"), ("来させる", "causative"),
            ("来よう", "volitional"),
            ("来ている", "progressive"), ("きていた", "progressive past"),
            ("来ています", "progressive polite"), ("来ていました", "progressive polite past"),
            ("来ていません", "progressive polite negative"),
        ];
    for (suffix, tag) in kuru_forms {
        rules.push(DeconjRule { strip_suffix: suffix.to_string(), append_suffix: "来る".to_string(), tag });
    }

    // ── i-adjectives ──
    let iadj_forms: &[(&str, &str)] = &[
        ("くない", "negative"), ("かった", "past"),
        ("くなかった", "past negative"), ("くて", "te-form"),
        ("すぎる", "too much"),
    ];
    for (suffix, tag) in iadj_forms {
        rules.push(DeconjRule { strip_suffix: suffix.to_string(), append_suffix: "い".to_string(), tag });
    }

    // ── copula / na-adjectives (no re-attached ending — bare noun/na-adj form) ──
    let copula_forms: &[(&str, &str)] = &[
        ("じゃない", "negative"), ("ではない", "negative"),
        ("だった", "past"), ("じゃなかった", "past negative"), ("ではなかった", "past negative"),
        ("でした", "polite past"),
        ("じゃありません", "polite negative"), ("ではありません", "polite negative"),
    ];
    for (suffix, tag) in copula_forms {
        rules.push(DeconjRule { strip_suffix: suffix.to_string(), append_suffix: String::new(), tag });
    }

    rules
}

/// Tries every rule against `text`, recursively re-applying rules to results
/// (up to `max_depth`) to catch stacked conjugations that aren't explicitly
/// enumerated as one compound suffix above.
pub fn deconjugate(text: &str, rules: &[DeconjRule], max_depth: usize) -> Vec<DeconjugatedForm> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    deconjugate_recursive(text, rules, max_depth, Vec::new(), &mut results, &mut seen);
    results
}

fn deconjugate_recursive(
    text: &str,
    rules: &[DeconjRule],
    depth_remaining: usize,
    chain: Vec<&'static str>,
    results: &mut Vec<DeconjugatedForm>,
    seen: &mut HashSet<String>,
) {
    if depth_remaining == 0 {
        return;
    }

    for rule in rules {
        // Fix: Use >= so suffix rules that cover the entire conjugated form work
        if text.len() >= rule.strip_suffix.len() && text.ends_with(rule.strip_suffix.as_str()) {
            let stem = &text[..text.len() - rule.strip_suffix.len()];
            let candidate = format!("{stem}{}", rule.append_suffix);

            if candidate == text || !seen.insert(candidate.clone()) {
                continue;
            }

            let mut new_chain = chain.clone();
            new_chain.push(rule.tag);
            results.push(DeconjugatedForm { text: candidate.clone(), rule_chain: new_chain.clone() });

            deconjugate_recursive(&candidate, rules, depth_remaining - 1, new_chain, results, seen);
        }
    }
}