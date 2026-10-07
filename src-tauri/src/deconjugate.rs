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
    "adj-i", "adj-ix", "adj-na", "adj-t", "adj-no", // <-- Added missing adjective types
    "cop", "v1", "v1-s", "v4r", "v5aru", "v5b", "v5g",
    "v5k", "v5k-s", "v5m", "v5n", "v5r", "v5r-i", "v5s", "v5t", "v5u",
    "v5u-s", "vk", "vs-c", "vs-i", "vs-s", "vz",
];

fn is_recordable_tag(tag: &str) -> bool {
    tag == "any" || VALID_WORD_CLASSES.contains(&tag)
}

/// Word classes of historical Japanese (see `Deconjugator::deconjugate_inner`).
fn is_archaic_tag(tag: &str) -> bool {
    matches!(tag, "v4r" | "vz" | "vs-c" | "adj-ix")
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
    /// Historical word classes are excluded (see `is_archaic_tag`); use
    /// `deconjugate_including_archaic` for the classical fallback.
    pub fn deconjugate(&self, text: &str) -> Vec<DeconjugatedForm> {
        self.deconjugate_inner(text, false)
    }

    /// Deconjugation including historical word classes (see
    /// `is_archaic_tag`). Used only by the classical fallback in lookup,
    /// which runs solely when nothing modern resolved.
    pub(crate) fn deconjugate_including_archaic(&self, text: &str) -> Vec<DeconjugatedForm> {
        self.deconjugate_inner(text, true)
    }

    /// Word classes of historical Japanese: Yodan with ru ending (v4r),
    /// zuru verbs (vz), the su-precursor to modern suru (vs-c), and archaic
    /// ku/shiku adjectives (adj-ix). Excluded from normal deconjugation so
    /// modern text never routes through them, while real classical forms
    /// stay reachable on demand.
    fn deconjugate_inner(&self, text: &str, allow_archaic: bool) -> Vec<DeconjugatedForm> {
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
                // the fewest proper steps for each (text, tag). Historical
                // classes record only on the archaic path: modern text must
                // never route through them (mixed chains still flow, since
                // only recording — never rule application — is gated).
                if let Some(tag) = &form.tag {
                    if is_recordable_tag(tag) && (allow_archaic || !is_archaic_tag(tag)) {
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

impl Deconjugator {
    /// The first (outermost) rule that applies to the raw surface, in engine
    /// application order. Names the surface's conjugation even when every rule
    /// path dead-ends before reaching a recordable dictionary form (e.g.
    /// はじめます -> "polite": the polite rule strips ます, but はじめ is an
    /// unrecordable stem class, so no deconjugation result carries the chain).
    pub fn first_rule(&self, text: &str) -> Option<String> {
        let form = FormState {
            text: text.to_string(),
            tag: None,
            original: true,
            proper_steps: 0,
            chain: Vec::new(),
        };
        for rule in rules_for(self, &form) {
            match rule.rule_type {
                RuleKind::OnlyFinal => {}
                RuleKind::NeverFinal => continue,
                RuleKind::Rewrite => {
                    if form.text != rule.con_end {
                        continue;
                    }
                }
                RuleKind::Std => {}
            }
            if form.text.len() == rule.con_end.len() && rule.dec_end.is_empty() {
                continue;
            }
            if !form.text.ends_with(&rule.con_end) {
                continue;
            }
            let detail = rule.detail.as_str();
            if detail.is_empty() {
                continue;
            }
            if detail.starts_with('(') {
                if detail.len() >= 2 && detail.ends_with(')') {
                    return Some(detail[1..detail.len() - 1].to_string());
                }
                return None;
            }
            return Some(detail.to_string());
        }
        None
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
/// notes ("(mizenkei)") are shown stripped when they are the first or the last
/// applied rule; middle parentheticals are skipped.
fn chain_description(chain: &[String]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for (i, detail) in chain.iter().enumerate() {
        if detail.is_empty() {
            continue;
        }
        if detail.starts_with('(') {
            if (i == 0 || i == chain.len() - 1) && detail.len() >= 2 {
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

    // Compound "must" suffixes (なければならない family, なくては/なくては
    // いけない family, and ないといけない family), applied to the godan rows,
    // ichidan, suru, and kuru. Extended beyond the formal set with polite-past
    // forms (いけませんでした), colloquial stems (なきゃ/なくちゃ/なくっちゃ/
    // ねば), and ないでは variants — otherwise the tail (いけない/ならない)
    // falls off into its own span in real sentences.
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
    let must_suffixes: &[&str] = &[
        "なければならない",
        "なければならなかった",
        "なければなりません",
        "なければいけない",
        "なければいけません",
        "なければいけなかった",
        "なくてはいけない",
        "なくてはいけません",
        "なくてはいけなかった",
        "なくてはならない",
        "なくてはなりません",
        "なくてはならなかった",
        "ないといけない",
        "ないといけません",
        "ないといけなかった",
    ];
    // Polite-past and なかったです forms missing from the formal set.
    let must_suffixes_polite: &[&str] = &[
        "なければなりませんでした",
        "なければいけませんでした",
        "なければならなかったです",
        "なくてはなりませんでした",
        "なくてはいけませんでした",
        "なくてはならなかったです",
        "ないといけませんでした",
    ];
    // Colloquial negative stems x standard endings. The godan/ichidan/suru
    // loops below attach these exactly like the formal ones (a-row kana +
    // suffix, bare suffix, し + suffix).
    let must_suffixes_colloquial: &[&str] = &[
        "なきゃいけない",
        "なきゃいけません",
        "なきゃいけなかった",
        "なきゃならない",
        "なきゃなりません",
        "なきゃならなかった",
        "なくちゃいけない",
        "なくちゃいけません",
        "なくちゃいけなかった",
        "なくちゃならない",
        "なくちゃなりません",
        "なくちゃならなかった",
        "なくっちゃいけない",
        "なくっちゃいけません",
        "なくっちゃいけなかった",
        "なくっちゃならない",
        "なくっちゃなりません",
        "なくっちゃならなかった",
        "ねばいけない",
        "ねばいけません",
        "ねばいけなかった",
        "ねばならない",
        "ねばなりません",
        "ねばならなかった",
    ];
    // ないでは variants (ないではいけない ≃ なくてはいけない).
    let must_suffixes_dewa: &[&str] = &[
        "ではいけない",
        "ではいけません",
        "ではいけなかった",
        "ではならない",
        "ではなりません",
        "ではならなかった",
    ];
    for (a, dict_ending) in godan_rows {
        for suffix in must_suffixes
            .iter()
            .chain(must_suffixes_polite.iter())
            .chain(must_suffixes_colloquial.iter())
            .chain(must_suffixes_dewa.iter())
        {
            let suffix: &str = suffix;
            rules.push(VirtualRule {
                rule_type: RuleKind::OnlyFinal,
                dec_end: dict_ending.to_string(),
                con_end: format!("{a}{suffix}"),
                dec_tag: "any".to_string(),
                con_tag: String::new(),
                detail: "must".to_string(),
            });
        }
    }

    for suffix in must_suffixes
        .iter()
        .chain(must_suffixes_polite.iter())
        .chain(must_suffixes_colloquial.iter())
        .chain(must_suffixes_dewa.iter())
    {
        let suffix: &str = suffix;
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "る".to_string(),
            con_end: suffix.to_string(),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "must".to_string(),
        });
    }

    for suffix in must_suffixes
        .iter()
        .chain(must_suffixes_polite.iter())
        .chain(must_suffixes_colloquial.iter())
        .chain(must_suffixes_dewa.iter())
    {
        let suffix: &str = suffix;
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "する".to_string(),
            con_end: format!("し{suffix}"),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "must".to_string(),
        });
    }

    // Kuru takes こ + suffix (来なければいけない etc.). The bare-suffix
    // ichidan loop above would otherwise resolve these to junk (こる).
    for suffix in must_suffixes
        .iter()
        .chain(must_suffixes_polite.iter())
        .chain(must_suffixes_colloquial.iter())
        .chain(must_suffixes_dewa.iter())
    {
        let suffix: &str = suffix;
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "くる".to_string(),
            con_end: format!("こ{suffix}"),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "must".to_string(),
        });
    }

    // "Even if (not)" — なくても and ても/でも/っても. JL has nothing for
    // the も-ending te-form, so 急がなくても would otherwise fall back to
    // naming an intermediate stem. OnlyFinal: the suffixes only ever apply
    // to the raw surface.
    for (a, dict_ending) in godan_rows {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: dict_ending.to_string(),
            con_end: format!("{a}なくても"),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "even if not".to_string(),
        });
    }
    for suffix in ["なくても"] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "る".to_string(),
            con_end: suffix.to_string(),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "even if not".to_string(),
        });
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "する".to_string(),
            con_end: format!("し{suffix}"),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "even if not".to_string(),
        });
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "くる".to_string(),
            con_end: format!("こ{suffix}"),
            dec_tag: "any".to_string(),
            con_tag: String::new(),
            detail: "even if not".to_string(),
        });
    }
    for (con_end, dec_end) in [("ても", "て"), ("でも", "で"), ("っても", "て")] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: dec_end.to_string(),
            con_end: con_end.to_string(),
            dec_tag: "stem-te".to_string(),
            con_tag: String::new(),
            detail: "even if".to_string(),
        });
    }
    // Adjective te-forms conjugate differently: 高くても -> 高く -> 高い.
    rules.push(VirtualRule {
        rule_type: RuleKind::OnlyFinal,
        dec_end: "く".to_string(),
        con_end: "くても".to_string(),
        dec_tag: "stem-ku".to_string(),
        con_tag: String::new(),
        detail: "even if".to_string(),
    });

    // Prohibitive "don't" (verb + んじゃない) and its past counterpart
    // "shouldn't have" (verb + んじゃなかった). ん is the explanatory の and
    // じゃ the では contraction, attached directly to the dictionary form:
    // 買うんじゃない -> 買う. OnlyFinal so the suffix only ever applies to the
    // raw surface, never to an intermediate deconjugated form, and "any"
    // keeps them POS-unrestricted at lookup time.
    let prohibitive: &[(&str, &str)] = &[
        ("んじゃない", "don't"),
        ("んじゃなかった", "shouldn't have"),
    ];
    for (_, dict_ending) in godan_rows {
        for (suffix, label) in prohibitive {
            rules.push(VirtualRule {
                rule_type: RuleKind::OnlyFinal,
                dec_end: dict_ending.to_string(),
                con_end: format!("{dict_ending}{suffix}"),
                dec_tag: "any".to_string(),
                con_tag: String::new(),
                detail: label.to_string(),
            });
        }
    }
    for (suffix, label) in prohibitive {
        // ichidan る, suru する, and kuru くる.
        for dict_ending in ["る", "する", "くる"] {
            rules.push(VirtualRule {
                rule_type: RuleKind::OnlyFinal,
                dec_end: dict_ending.to_string(),
                con_end: format!("{dict_ending}{suffix}"),
                dec_tag: "any".to_string(),
                con_tag: String::new(),
                detail: label.to_string(),
            });
        }
    }

    // Casual-contracted imperatives ～ちゃいな/じゃいな (準備しちゃいなよ)
    // and their full ～ちゃいなさい/じゃいなさい forms: てしまいなさい
    // with the いなさい compressed. Reduce straight to てしまう/
    // でしまう (v5u, like JL's own ちゃう rule) so the chain continues to
    // the verb normally (しちゃいな -> してしまう -> する).
    for (con_end, dec_end, detail) in [
        ("ちゃいな", "てしまう", "contracted + casual imperative"),
        ("じゃいな", "でしまう", "contracted + casual imperative"),
        ("ちゃいなさい", "てしまう", "contracted + polite imperative"),
        ("じゃいなさい", "でしまう", "contracted + polite imperative"),
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: dec_end.to_string(),
            con_end: con_end.to_string(),
            dec_tag: "v5u".to_string(),
            con_tag: String::new(),
            detail: detail.to_string(),
        });
    }

    // Adjective causative/passive (悪くさせて -> 悪い): the ku-stem +
    // させる/される family. The causative auxiliary inflects as ichidan,
    // so every common inflection needs its own suffix; all reduce to the
    // bare い stem (POS-validated to adjectives at lookup time, tag
    // adj-i). しく-stems are covered automatically (美しくさせる ends in
    // くさせる -> 美しい), as is よく (よくさせる -> よい).
    for (con_end, detail) in [
        ("くさせる", "causative"),
        ("くさせて", "causative + te"),
        ("くさせた", "causative + past"),
        ("くさせない", "causative + negative"),
        ("くさせます", "causative + polite"),
        ("くさせよう", "causative + volitional"),
        ("くさせろ", "causative + imperative"),
        ("くさせるな", "causative + prohibitive"),
        ("くさせたい", "causative + want to"),
        ("くさせられる", "causative + passive/potential"),
        ("くさせられた", "causative + passive/potential + past"),
        ("くされる", "passive"),
        ("くされて", "passive + te"),
        ("くされた", "passive + past"),
        ("くされない", "passive + negative"),
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: "い".to_string(),
            con_end: con_end.to_string(),
            dec_tag: "adj-i".to_string(),
            con_tag: String::new(),
            detail: detail.to_string(),
        });
    }

    // Negative-hearsay なさそう (考えてなさそう -> 考える): ない-stem
    // な + さ + hearsay そう, for which JL has no rule. Three
    // non-overlapping surface forms so verb te-stems, verb masu-stems,
    // and adjective ku-stems each continue through their own JL rules
    // without label pollution: 考えて (stem-te), 食べ (stem-ren),
    // 寒く -> 寒い (adj-i, like the adjective-causative rules).
    let nasa_suffixes: &[(&str, &str)] = &[
        ("", "negative + hearsay"),
        ("だ", "negative + hearsay"),
        ("だった", "negative + hearsay + past"),
        ("です", "negative + hearsay + polite"),
        ("な", "negative + hearsay"),
        ("に", "negative + hearsay"),
    ];
    for (base, dec_end, dec_tag) in [
        ("てなさそう", "て", "stem-te"),
        ("なさそう", "", "stem-ren"),
        ("くなさそう", "い", "adj-i"),
    ] {
        for (suffix, detail) in nasa_suffixes {
            rules.push(VirtualRule {
                rule_type: RuleKind::OnlyFinal,
                dec_end: dec_end.to_string(),
                con_end: format!("{base}{suffix}"),
                dec_tag: dec_tag.to_string(),
                con_tag: String::new(),
                detail: detail.to_string(),
            });
        }
    }

    // なさい after non-verb stems (ゴメンなさい): JL's version decays to a
    // masu-stem tag, which only continues for verb stems — a noun stem
    // dead-ends invisibly. This reduces to an empty stem with an
    // unrestricted tag so the noun itself resolves (ゴメン -> 御免); verb
    // stems are unaffected (ます-stem paths still win, and し-stems resolve
    // via morphology first, outranking by kind). Detail mirrors JL's own
    // rule so labels stay consistent by path. (ください deliberately left
    // out: 教えてください already resolves, and an any-stem ください would
    // let 教え-noun outstep 教える on fewer steps.)
    rules.push(VirtualRule {
        rule_type: RuleKind::OnlyFinal,
        dec_end: String::new(),
        con_end: "なさい".to_string(),
        dec_tag: "any".to_string(),
        con_tag: String::new(),
        detail: "polite imperative".to_string(),
    });

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

    // Listing たり/だり on the copula (自主トレだったり, 彼からだったり, でしたり).
    // The copula has no tari rule of its own: JL routes だったり through the
    // godan-verb tari rules (だったる / だつ), which the verb-class gate then
    // rejects because a copula span carries no 動詞 token — so the span fell
    // back to だった plus a stray り (利). Two shapes, mirroring the copula
    // suffixes above: with a stem the noun/particle absorbs the whole copula
    // (自主トレだったり -> 自主トレ, like 自主トレだった), bare it resolves to
    // the copula itself (だったり -> だ). OnlyFinal, so verbs ending in a
    // coincidental だったり are unaffected — no verb surfaces like that.
    for (con_end, dec_end, dec_tag, detail) in [
        ("だったり", "", "any", "copula + tari"),
        ("だったり", "だ", "cop", "past + tari"),
        ("でしたり", "", "any", "copula + tari"),
        ("でしたり", "です", "cop", "past + tari"),
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: dec_end.to_string(),
            con_end: con_end.to_string(),
            dec_tag: dec_tag.to_string(),
            con_tag: String::new(),
            detail: detail.to_string(),
        });
    }

    // Freshly-finished たて (転校したて -> する, 淹れ立て -> 淹れる):
    // the masu-stem takes たて to form a noun meaning "just done".
    // One rule per stem shape so the dictionary form restores exactly
    // (ichidan stem + る, godan stem + its row vowel, する/くる special
    // cases). Rendaku だて included (出来立て = できだて -> できる).
    // OnlyFinal, so verbs never pick the suffix up mid-chain.
    for (con_end, dec_end, dec_tag) in [
        ("たて", "る", "v1"),
        ("だて", "る", "v1"),
        ("いたて", "う", "v5u"),
        ("いだて", "う", "v5u"),
        ("きたて", "く", "v5k"),
        ("きだて", "く", "v5k"),
        ("ぎたて", "ぐ", "v5g"),
        ("ぎだて", "ぐ", "v5g"),
        ("したて", "す", "v5s"),
        ("しだて", "す", "v5s"),
        ("ちたて", "つ", "v5t"),
        ("ちだて", "つ", "v5t"),
        ("にたて", "ぬ", "v5n"),
        ("にだて", "ぬ", "v5n"),
        ("びたて", "ぶ", "v5b"),
        ("びだて", "ぶ", "v5b"),
        ("みたて", "む", "v5m"),
        ("みだて", "む", "v5m"),
        ("りたて", "る", "v5r"),
        ("りだて", "る", "v5r"),
        ("したて", "する", "vs-i"),
        ("しだて", "する", "vs-i"),
        ("こたて", "くる", "vk"),
        ("こだて", "くる", "vk"),
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: dec_end.to_string(),
            con_end: con_end.to_string(),
            dec_tag: dec_tag.to_string(),
            con_tag: String::new(),
            detail: "right after doing".to_string(),
        });
    }

    // Plain renyoukei (masu-stem) to dictionary form for ru-verbs
    // (あり -> ある, おり -> おる) and kuru's き (き -> くる): JL only
    // reaches these through polite ます-forms, so a bare stem lists
    // homophones (あり -> 蟻) but never the verb. "continuative" names the
    // stem in the tooltip. OnlyFinal and POS-gated; same-entry dedupe
    // keeps longer chains stable, and literal kinds still outrank the new
    // candidates (the morphological path promotes the verb instead).
    for (con_end, dec_end, dec_tag) in [
        ("り", "る", "v5r"),
        ("り", "る", "v5r-i"),
        ("き", "くる", "vk"),
        // Causative continuative (待たせ -> 待たせる, させ -> させる):
        // UniDic reports the lexicalized causative stem as the base
        // (IPAdic reported the root), so these stems only resolve through
        // this. v1: all せる-verbs are ichidan. The emphatic-sokuon twin
        // (待たせっ) covers clipped slang the same way.
        ("せ", "せる", "v1"),
        ("せっ", "せる", "v1"),
    ] {
        rules.push(VirtualRule {
            rule_type: RuleKind::OnlyFinal,
            dec_end: dec_end.to_string(),
            con_end: con_end.to_string(),
            dec_tag: dec_tag.to_string(),
            con_tag: String::new(),
            detail: "continuative".to_string(),
        });
    }
    // Negative continuative into na-adjective entries (とりつく島もなく
    // -> 取り付く島もない, 仕方なく -> 仕方ない): the generic く->い
    // adjective rule reaches these with an adj-i tag, which can never
    // validate against keiyodoshi entries — so the idiom dies and the
    // span shreds. adj-na is POS-unrestricted at lookup, but the form
    // only enters the pool when it spells a real entry, so 食べなく
    // and friends still fall through to the verb. Parenthetical detail
    // keeps the label clean (the expression is the answer, not the
    // inflection).
    rules.push(VirtualRule {
        rule_type: RuleKind::OnlyFinal,
        dec_end: "ない".to_string(),
        con_end: "なく".to_string(),
        dec_tag: "adj-na".to_string(),
        con_tag: String::new(),
        detail: "(continuative)".to_string(),
    });

    // Past of the とく contraction on する (しといた -> する, like しとく
    // -> する via JL's toku rule): といた has no JL rule, so the span
    // died and しといた never resolved. Direct to する (vs-i); other
    // verbs' といた surfaces decay to misses as before.
    rules.push(VirtualRule {
        rule_type: RuleKind::OnlyFinal,
        dec_end: "する".to_string(),
        con_end: "しといた".to_string(),
        dec_tag: "vs-i".to_string(),
        con_tag: String::new(),
        detail: "toku (for now)".to_string(),
    });

    // したい -> する ("want to", vs-i): JL has no したい rule, so bare
    // したい lists nouns (したい -> 死体) and どうしたい degrades to
    // どうし. Full-form only (con したい, never bare たい — 食べたい
    // must keep reaching 食べる, not 食べする); Xしたい surfaces decay
    // to misses unless Xする is itself an entry (勉強したい -> 勉強する,
    // which is the right answer there too).
    rules.push(VirtualRule {
        rule_type: RuleKind::OnlyFinal,
        dec_end: "する".to_string(),
        con_end: "したい".to_string(),
        dec_tag: "vs-i".to_string(),
        con_tag: String::new(),
        detail: "want to".to_string(),
    });

    rules
}

/// How an entry was reached for a given surface. Used as a tie-breaker so a
/// Maps a JL/Nazeka deconjugation tag to the JMdict English POS labels an
/// entry must carry for the deconjugated result to be valid (JL's
/// GetValidDeconjugatedResults). "any" (tomoyo's supplementary rules) and
/// unknown tags are POS-unrestricted.
pub(crate) fn deconj_tag_to_dict_pos(tag: &str) -> &'static [&'static str] {
    match tag {
        "v1" => &["Ichidan verb"],
        "v1-s" => &["Ichidan verb - kureru special class"],
        "v4r" => &["Yodan verb with 'ru' ending (archaic)"],
        "v5aru" => &["Godan verb - -aru special class"],
        "v5b" => &["Godan verb with 'bu' ending"],
        "v5g" => &["Godan verb with 'gu' ending"],
        "v5k" => &["Godan verb with 'ku' ending"],
        "v5k-s" => &["Godan verb - Iku/Yuku special class"],
        "v5m" => &["Godan verb with 'mu' ending"],
        "v5n" => &["Godan verb with 'nu' ending"],
        "v5r" => &["Godan verb with 'ru' ending"],
        "v5r-i" => &["Godan verb with 'ru' ending (irregular verb)"],
        "v5s" => &["Godan verb with 'su' ending"],
        "v5t" => &["Godan verb with 'tsu' ending"],
        "v5u" => &["Godan verb with 'u' ending"],
        "v5u-s" => &["Godan verb with 'u' ending (special class)"],
        "vk" => &["Kuru verb - special class"],
        "vs-c" => &["su verb - precursor to the modern suru"],
        "vs-i" => &["suru verb - included"],
        "vs-s" => &["suru verb - special class"],
        "vz" => &["Ichidan verb - zuru verb (alternative form of -jiru verbs)"],
        "adj-i" => &["adjective (keiyoushi)"],
        "adj-ix" => &["'ku' adjective (archaic)", "'shiku' adjective (archaic)"],
        "cop" => &["copula"],
        _ => &[],
    }
}

pub(crate) fn deconj_tag_matches_entry(entry_pos: &[String], tag: &str) -> bool {
    let allowed = deconj_tag_to_dict_pos(tag);
    allowed.is_empty() || entry_pos.iter().any(|p| allowed.contains(&p.as_str()))
}

/// Verb word classes the deconjugation rules can claim. Deconjugation
/// results in one of these are only trusted when the span actually contains
/// a 動詞 token — otherwise a noun/na-adjective + な (好きな) deconjugates
/// through the imperative な rule into a coincidental verb (好く).
pub(crate) fn is_verb_class(tag: &str) -> bool {
    matches!(
        tag,
        "v1" | "v1-s"
            | "v4r"
            | "v5aru"
            | "v5b"
            | "v5g"
            | "v5k"
            | "v5k-s"
            | "v5m"
            | "v5n"
            | "v5r"
            | "v5r-i"
            | "v5s"
            | "v5t"
            | "v5u"
            | "v5u-s"
            | "vk"
            | "vs-c"
            | "vs-i"
            | "vs-s"
            | "vz"
    )
}

/// Jargon-y JL rule names for sound changes and auxiliary helpers that say
/// nothing about the surface form — excluded from combined labels so
/// してくれました reads "polite past" rather than "polite past +
/// statement/request + unstressed infinitive".
pub(crate) fn is_stem_jargon(detail: &str) -> bool {
    matches!(
        detail,
        // Auxiliary helpers and sound changes that say nothing about the
        // surface form.
        "statement/request"
            | "slurred"
            | "slurred negative"
            | "rough casual"
            | "ksb"
            | "contracted"
            // JL's parenthetical stem notes, which chain_description has
            // already stripped of their parentheses.
            | "masu stem"
            | "unstressed infinitive"
            | "stem"
            | "adverbial stem"
            | "izenkei"
            | "ka stem"
            | "ke stem"
            | "mizenkei"
            | "'a' stem"
    )
}

/// Shorter, plainer names for verbose JL rule details. Ambiguous られる
/// forms (passive / potential / honorific for ichidan+くる) stay labeled
/// "passive/potential" at a glance instead of collapsing to "potential" and
/// hiding the passive reading from learners.
pub(crate) fn curated_name(detail: &str) -> &str {
    match detail {
        "finish/completely/end up" => "ended up",
        "passive/potential/honorific" | "passive/potential" => "passive/potential",
        "toku (for now)" => "in advance (casual)",
        other => other,
    }
}

/// Names the surface's conjugation from a deconjugation rule chain by
/// combining every meaningful rule that applied, outermost first — e.g.
/// 住んでいた -> "past + teiru", 忘れてしまった -> "past + ended up".
/// Parenthetical stem notes ("(masu stem)") are skipped, except a leading
/// one like (te) in 飲んで, which is the surface conjugation itself.
pub(crate) fn combined_label(chain: &str) -> Option<String> {
    let build_parts = |skip_leading_te: bool| -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for (i, detail) in chain.split('→').enumerate() {
            if detail.is_empty() {
                continue;
            }
            if detail.starts_with('(') {
                if i == 0 && detail.len() >= 2 && detail.ends_with(')') {
                    parts.push(detail[1..detail.len() - 1].to_string());
                }
                continue;
            }
            // A leading て/で is just the carrier for a trailing conjugation
            // morpheme (できて -> "potential", 忘れておく -> "in advance").
            // It is only dropped when something meaningful follows: 寝てた's
            // chain "te→unstressed infinitive" has only jargon after the て,
            // so there the て itself names the surface.
            if skip_leading_te && i == 0 && (detail == "te" || detail == "de") {
                continue;
            }
            if is_stem_jargon(detail) {
                continue;
            }
            parts.push(curated_name(detail).to_string());
        }
        parts
    };
    let mut parts = build_parts(true);
    if parts.is_empty() {
        parts = build_parts(false);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" + "))
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn archaic_forms_stay_behind_the_flag() {
        let decon =
            Deconjugator::build(include_str!("../resources/deconjugation_rules.json"));
        let modern = decon.deconjugate("ろんぜず");
        assert!(
            !modern
                .iter()
                .any(|f| ["v4r", "vz", "vs-c", "adj-ix"].contains(&f.tag.as_str())),
            "modern deconjugation must not produce archaic word classes"
        );
        let full = decon.deconjugate_including_archaic("ろんぜず");
        assert!(
            full.iter().any(|f| f.text == "ろんずる" && f.tag == "vz"),
            "archaic path must reach ろんずる/vz; got {:?}",
            full.iter()
                .map(|f| (f.text.clone(), f.tag.clone()))
                .collect::<Vec<_>>()
        );
    }
}
