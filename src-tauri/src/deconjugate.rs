use std::collections::HashMap;

/// Rule kind, mirroring JL/Nazeka's `deconjugation_rules.json`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize)]
pub enum RuleKind {
    #[serde(rename = "stdrule")]
    Std,
    #[serde(rename = "onlyfinalrule")]
    OnlyFinal,
    #[serde(rename = "neverfinalrule")]
    NeverFinal,
    #[serde(rename = "rewriterule")]
    Rewrite,
}

/// One rule as it appears in the JSON: arrays are parallel pairs (a single
/// element in a tag array applies to every con/dec pair).
#[derive(Clone, Debug, serde::Deserialize)]
struct RawRule {
    #[serde(rename = "type")]
    rule_type: RuleKind,
    #[serde(rename = "dec_end")]
    dec_end: Vec<String>,
    #[serde(rename = "con_end")]
    con_end: Vec<String>,
    #[serde(rename = "dec_tag")]
    dec_tag: Vec<String>,
    #[serde(rename = "con_tag")]
    con_tag: Vec<String>,
    detail: String,
}

/// A single concrete rule: one (con_end, dec_end) pair with its tags.
#[derive(Clone, Debug)]
struct VirtualRule {
    rule_type: RuleKind,
    dec_end: String,
    con_end: String,
    dec_tag: String,
    con_tag: String,
    detail: String,
}

/// A deconjugated form of a surface: the resolved dictionary-form text, the
/// word class the rule chain produced (used for POS validation at lookup),
/// a human-readable rule chain, and how many proper steps it took.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DeconjugatedForm {
    pub text: String,
    pub tag: String,
    pub rule_chain: Option<String>,
    pub proper_steps: usize,
}

#[derive(Default)]
struct RuleBucket {
    /// Every rule in the bucket — tried against untagged (initial) forms.
    all_rules: Vec<VirtualRule>,
    /// Rules keyed by their con_tag — tried against tagged intermediate forms.
    by_con_tag: HashMap<String, Vec<VirtualRule>>,
}

impl RuleBucket {
    fn push(&mut self, rule: VirtualRule) {
        self.by_con_tag
            .entry(rule.con_tag.clone())
            .or_default()
            .push(rule.clone());
        self.all_rules.push(rule);
    }
}

/// JL's Nazeka-based deconjugation engine, driven by
/// `deconjugation_rules.json` (mirrors JL's `Deconjugator`/`DeconjugatorUtils`).
pub struct Deconjugator {
    buckets: HashMap<char, RuleBucket>,
    empty_con_end: RuleBucket,
}

/// JMdict word classes that count as real dictionary-form results. JL's
/// intermediate stem tags ("stem-mizenkei", "masu stem", "te", ...) are never
/// recorded — only these final word classes are, plus tomoyo's POS-unrestricted
/// supplement tag "any".
const VALID_WORD_CLASSES: &[&str] = &[
    "adj-i", "adj-ix", "cop", "v1", "v1-s", "v4r", "v5aru", "v5b", "v5g",
    "v5k", "v5k-s", "v5m", "v5n", "v5r", "v5r-i", "v5s", "v5t", "v5u",
    "v5u-s", "vk", "vs-c", "vs-i", "vs-s", "vz",
];

fn is_recordable_tag(tag: &str) -> bool {
    tag == "any" || VALID_WORD_CLASSES.contains(&tag)
}

const MAX_PROPER_STEPS: usize = 7;

impl Deconjugator {
    pub fn build(rules_json: &str) -> Self {
        let raw: Vec<RawRule> =
            serde_json::from_str(rules_json).expect("invalid deconjugation_rules.json");

        let mut virtual_rules: Vec<VirtualRule> = Vec::new();
        for r in raw {
            let single_con_tag = match r.con_tag.len() {
                1 => Some(r.con_tag[0].clone()),
                _ => None,
            };
            let single_dec_tag = match r.dec_tag.len() {
                1 => Some(r.dec_tag[0].clone()),
                _ => None,
            };
            for i in 0..r.con_end.len() {
                let con_tag = single_con_tag
                    .clone()
                    .unwrap_or_else(|| r.con_tag[i].clone());
                let dec_tag = single_dec_tag
                    .clone()
                    .unwrap_or_else(|| r.dec_tag[i].clone());
                virtual_rules.push(VirtualRule {
                    rule_type: r.rule_type,
                    dec_end: r.dec_end[i].clone(),
                    con_end: r.con_end[i].clone(),
                    dec_tag,
                    con_tag,
                    detail: r.detail.clone(),
                });
            }
        }

        virtual_rules.extend(supplemental_rules());

        let mut buckets: HashMap<char, RuleBucket> = HashMap::new();
        let mut empty_con_end = RuleBucket::default();
        for rule in virtual_rules {
            match rule.con_end.chars().next_back() {
                Some(last) => buckets.entry(last).or_default().push(rule),
                None => empty_con_end.push(rule),
            }
        }

        Self { buckets, empty_con_end }
    }

    /// Deconjugates `text` (expected already normalized to hiragana) into all
    /// recorded dictionary-form results, keeping the fewest-step chain per
    /// (text, word class) — mirroring JL's `Deconjugator.Deconjugate`.
    pub fn deconjugate(&self, text: &str) -> Vec<DeconjugatedForm> {
        let mut results: Vec<DeconjugatedForm> = Vec::new();
        // Queue dedup keyed by (text, tag): keep the fewest proper steps so
        // downstream forms always branch from the shortest valid chain.
        let mut best: HashMap<(String, String), usize> = HashMap::new();
        let mut queue: Vec<FormState> = vec![FormState {
            text: text.to_string(),
            tag: None,
            original: true,
            proper_steps: 0,
            chain: Vec::new(),
        }];

        while !queue.is_empty() {
            let mut next: Vec<FormState> = Vec::new();
            for form in &queue {
                for rule in rules_for(self, form) {
                    match rule.rule_type {
                        RuleKind::OnlyFinal => {
                            if form.tag.is_some() {
                                continue;
                            }
                        }
                        RuleKind::NeverFinal => {
                            if form.tag.is_none() {
                                continue;
                            }
                        }
                        RuleKind::Rewrite => {
                            if form.text != rule.con_end {
                                continue;
                            }
                        }
                        RuleKind::Std => {}
                    }

                    // Never strip the whole surface down to nothing.
                    if form.text.len() == rule.con_end.len() && rule.dec_end.is_empty() {
                        continue;
                    }
                    // Too many proper deconjugation steps.
                    if form.proper_steps > MAX_PROPER_STEPS {
                        continue;
                    }
                    if !form.text.ends_with(&rule.con_end) {
                        continue;
                    }

                    let stem = &form.text[..form.text.len() - rule.con_end.len()];
                    let new_text = format!("{stem}{}", rule.dec_end);

                    // JL's ProcessNode: the first applied rule always counts as
                    // one proper step; later steps count unless the detail is a
                    // parenthetical stem note (e.g. "(mizenkei)").
                    let proper_steps = if form.original {
                        1
                    } else {
                        form.proper_steps + usize::from(!rule.detail.starts_with('('))
                    };

                    let mut chain = form.chain.clone();
                    chain.push(rule.detail.clone());

                    let key = (new_text.clone(), rule.dec_tag.clone());
                    let better = match best.get(&key) {
                        Some(&existing) => existing > proper_steps,
                        None => true,
                    };
                    if better {
                        best.insert(key, proper_steps);
                        next.push(FormState {
                            text: new_text,
                            tag: Some(rule.dec_tag.clone()),
                            original: false,
                            proper_steps,
                            chain,
                        });
                    }
                }

                // Record results in valid word classes, keeping the form with
                // the fewest proper steps for each (text, tag).
                if let Some(tag) = &form.tag {
                    if is_recordable_tag(tag) {
                        let (text, proper_steps) = (form.text.clone(), form.proper_steps);
                        let description = chain_description(&form.chain);
                        match results.iter_mut().find(|f| f.text == text && f.tag == *tag) {
                            Some(existing) => {
                                if proper_steps < existing.proper_steps {
                                    existing.proper_steps = proper_steps;
                                    existing.rule_chain = description;
                                }
                            }
                            None => results.push(DeconjugatedForm {
                                text,
                                tag: tag.clone(),
                                rule_chain: description,
                                proper_steps,
                            }),
                        }
                    }
                }
            }
            queue = next;
        }

        results
    }
}

fn rules_for<'a>(
    decon: &'a Deconjugator,
    form: &FormState,
) -> Vec<&'a VirtualRule> {
    let mut out: Vec<&'a VirtualRule> = Vec::new();
    let last_char = form.text.chars().next_back();
    match &form.tag {
        None => {
            if let Some(last) = last_char {
                if let Some(bucket) = decon.buckets.get(&last) {
                    out.extend(bucket.all_rules.iter());
                }
            }
            out.extend(decon.empty_con_end.all_rules.iter());
        }
        Some(tag) => {
            if let Some(last) = last_char {
                if let Some(bucket) = decon.buckets.get(&last) {
                    if let Some(rules) = bucket.by_con_tag.get(tag) {
                        out.extend(rules.iter());
                    }
                }
            }
            if let Some(rules) = decon.empty_con_end.by_con_tag.get(tag) {
                out.extend(rules.iter());
            }
        }
    }
    out
}

/// The queue/record state for a single deconjugation run. Defined at module
/// level so `rules_for` can take it without a nested-type lifetime mess.
struct FormState {
    text: String,
    tag: Option<String>,
    original: bool,
    proper_steps: usize,
    chain: Vec<String>,
}

/// Formats JL's process-node chain (newest detail first). Parenthetical stem
/// notes ("(mizenkei)") are shown stripped only when they are the very first
/// applied rule; other parentheticals are skipped.
fn chain_description(chain: &[String]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for (i, detail) in chain.iter().enumerate() {
        if detail.is_empty() {
            continue;
        }
        if detail.starts_with('(') {
            if i == chain.len() - 1 && detail.len() >= 2 {
                parts.push(detail[1..detail.len() - 1].to_string());
            }
        } else {
            parts.push(detail.clone());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("→"))
    }
}

/// tomoyo-specific compound suffixes layered on top of JL's engine: JL's rule
/// set does not resolve なければならない / なくてはいけない back to the main
/// verb, nor noun+だった copula forms. The "any" tag is POS-unrestricted at
/// lookup time. All are OnlyFinal so they only ever apply to the original
/// surface, never to an intermediate deconjugated form.
fn supplemental_rules() -> Vec<VirtualRule> {
    let mut rules = Vec::new();

    let godan_rows: &[(&str, &str)] = &[
        ("わ", "う"),
        ("か", "く"),
        ("が", "ぐ"),
        ("さ", "す"),
        ("た", "つ"),
        ("な", "ぬ"),
        ("ば", "ぶ"),
        ("ま", "む"),
        ("ら", "る"),
    ];
    for (a, dict_ending) in godan_rows {
        for suffix in [
            format!("{a}なければならない"),
            format!("{a}なければならなかった"),
            format!("{a}なければなりません"),
            format!("{a}なくてはいけない"),
        ] {
            rules.push(VirtualRule {
                rule_type: RuleKind::OnlyFinal,
                dec_end: dict_ending.to_string(),
                con_end: suffix,
                dec_tag: "any".to_string(),
                con_tag: String::new(),
                detail: "must".to_string(),
            });
        }
    }

    for suffix in [
        "なければならない",
        "なければならなかった",
        "なければなりません",
        "なくてはいけない",
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "る".to_string(),
            con_end: suffix.to_string(),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "must".to_string(),
        });
    }

    for suffix in [
        "しなければならない",
        "しなければならなかった",
        "しなければなりません",
        "しなくてはいけない",
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "する".to_string(),
            con_end: suffix.to_string(),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "must".to_string(),
        });
    }

    for suffix in [
        "じゃない",
        "ではない",
        "だった",
        "じゃなかった",
        "ではなかった",
        "でした",
        "じゃありません",
        "ではありません",
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: String::new(),
            con_end: suffix.to_string(),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "copula".to_string(),
        });
    }

    rules
}
