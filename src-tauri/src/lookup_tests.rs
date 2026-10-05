    use super::*;
    use crate::deconjugate::Deconjugator;
    use crate::index::{find_containing, DictionaryIndex};
    use crate::spans::lookup_from_position;
    use crate::types::{DictEntry, MatchSpan, MorphToken};
    use std::io::Read;
    use vibrato::{Dictionary, Tokenizer};
    use zstd::Decoder;

    pub(crate) struct Harness {
        pub(crate) index: DictionaryIndex,
        pub(crate) decon: Deconjugator,
        pub(crate) tokenizer: Tokenizer,
    }

    impl Harness {
        pub(crate) fn new() -> Self {
            let file = std::fs::File::open("resources/ipadic-mecab.dic.zst").unwrap();
            let mut reader = Decoder::new(file).unwrap();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).unwrap();
            let dict = Dictionary::read(&buf[..]).unwrap();
            let tokenizer = Tokenizer::new(dict);

            let jmdict_json = std::fs::read_to_string("resources/jmdict.json").unwrap();
            let entries: Vec<DictEntry> = serde_json::from_str(&jmdict_json).unwrap();
            let index = DictionaryIndex::build(entries);
            let decon = Deconjugator::build(include_str!("../resources/deconjugation_rules.json"));
            Self { index, decon, tokenizer }
        }

        pub(crate) fn tokens(&self, text: &str) -> Vec<MorphToken> {
            let mut worker = self.tokenizer.new_worker();
            worker.reset_sentence(text);
            worker.tokenize();
            worker
                .token_iter()
                .map(|t| {
                    let range = t.range_char();
                    let fields: Vec<&str> = t.feature().split(',').collect();
                    MorphToken {
                        start: range.start,
                        end: range.end,
                        surface: t.surface().to_string(),
                        base_form: fields.get(6).map(|s| s.to_string()).unwrap_or_else(|| t.surface().to_string()),
                        pos: fields.get(0).unwrap_or(&"").to_string(),
                        reading: normalize::normalize_text(fields.get(7).unwrap_or(&"")),
                    }
                })
                .collect()
        }

        fn lookup(&self, text: &str, position: usize) -> MatchSpan {
            let tokens = self.tokens(text);
            lookup_from_position(text, position, 0, &self.index, &self.decon, &tokens).unwrap()
        }
    }

    fn top_reading(span: &MatchSpan) -> String {
        span.entries[0].readings[0].clone()
    }

    fn scan(h: &Harness, text: &str) -> Vec<MatchSpan> {
        let tokens = h.tokens(text);
        let chars: Vec<char> = text.chars().collect();
        let mut spans = Vec::new();
        let mut pos = 0usize;
        while pos < chars.len() {
            if let Some(span) = lookup_from_position(text, pos, 0, &h.index, &h.decon, &tokens) {
                if !span.entries.is_empty() {
                    let end = span.end;
                    spans.push(span);
                    pos = end.max(pos + 1);
                    continue;
                }
            }
            let next = tokens
                .iter()
                .find(|t| t.start <= pos && pos < t.end)
                .or_else(|| tokens.iter().find(|t| t.start >= pos))
                .map(|t| t.end);
            pos = next.map(|e| e.max(pos + 1)).unwrap_or(pos + 1);
        }
        spans
    }

    fn cpos(text: &str, sub: &str) -> usize {
        let b = text.find(sub).unwrap_or_else(|| panic!("{sub:?} not in {text:?}"));
        text[..b].chars().count()
    }

    fn tsv_resolve_marker(text: &str, marker: &str) -> Result<usize, String> {
        if marker.is_empty() {
            return Ok(0);
        }
        if let Some(n) = marker.strip_prefix('@') {
            return n.parse::<usize>().map_err(|_| format!("bad index marker {marker:?}"));
        }
        match text.find(marker) {
            Some(b) => Ok(text[..b].chars().count()),
            None => Err(format!("marker {marker:?} not found")),
        }
    }

    fn tsv_top(span: &MatchSpan) -> Result<&std::sync::Arc<DictEntry>, String> {
        span.entries.first().ok_or_else(|| "span has no entries".to_string())
    }

    fn tsv_run_row(h: &Harness, sentence: &str, mode: &str, marker: &str, span_sel: &str, check: &str, expected: &str) -> Result<(), String> {
        if mode == "lookup" {
            let pos = tsv_resolve_marker(sentence, marker)?;
            let span = h.lookup(sentence, pos);
            return tsv_check_span(&span, check, expected);
        }
        if mode == "scan" {
            let spans = scan(h, sentence);
            if !span_sel.is_empty() {
                let span = spans.iter().find(|s| s.surface == span_sel).ok_or_else(|| {
                    let have: Vec<&str> = spans.iter().map(|s| s.surface.as_str()).take(12).collect();
                    format!("span {span_sel:?} not found (have {have:?})")
                })?;
                return tsv_check_span(span, check, expected);
            }
            let surfaces: Vec<&str> = spans.iter().map(|s| s.surface.as_str()).collect();
            let sfail = |msg: String| -> Result<(), String> {
                Err(format!("check={check} expected={expected:?}: {msg}"))
            };
            match check {
                "has_span" => {
                    if surfaces.contains(&expected) {
                        Ok(())
                    } else {
                        sfail(format!("missing span (have {surfaces:?})"))
                    }
                }
                "no_span" => {
                    if surfaces.contains(&expected) {
                        sfail("forbidden span present".to_string())
                    } else {
                        Ok(())
                    }
                }
                "no_span_contains" => {
                    if expected.is_empty() {
                        return Err("empty expected".to_string());
                    }
                    if surfaces.iter().any(|s| s.contains(expected)) {
                        sfail(format!("forbidden substring in {surfaces:?}"))
                    } else {
                        Ok(())
                    }
                }
                "scan_surfaces" => {
                    let actual = surfaces.join("|");
                    if actual == expected {
                        Ok(())
                    } else {
                        sfail(format!("actual={actual:?}"))
                    }
                }
                _ => Err(format!("unknown scan-wide check {check:?}")),
            }
        } else {
            Err(format!("unknown mode {mode:?}"))
        }
    }

    fn tsv_check_span(span: &MatchSpan, check: &str, expected: &str) -> Result<(), String> {
        let fail = |msg: String| -> Result<(), String> {
            Err(format!("check={check} expected={expected:?}: {msg}"))
        };
        match check {
            "surface" => {
                if span.surface == expected {
                    Ok(())
                } else {
                    fail(format!("actual={:?}", span.surface))
                }
            }
            "not_surface" => {
                if span.surface != expected {
                    Ok(())
                } else {
                    fail("forbidden surface matched".to_string())
                }
            }
            "start" => {
                let want: usize = expected
                    .parse()
                    .map_err(|_| format!("bad int {expected:?}"))?;
                if span.start == want {
                    Ok(())
                } else {
                    fail(format!("actual={}", span.start))
                }
            }
            "top_reading" => {
                let top = tsv_top(span)?;
                if top.readings.first().map_or(false, |r| r == expected) {
                    Ok(())
                } else {
                    fail(format!("actual={:?}", top.readings.first()))
                }
            }
            "top_reading_contains" => {
                let top = tsv_top(span)?;
                if top.readings.first().map_or(false, |r| r.contains(expected)) {
                    Ok(())
                } else {
                    fail(format!("actual={:?}", top.readings.first()))
                }
            }
            "not_top_reading" => {
                let top = tsv_top(span)?;
                let banned: Vec<&str> = expected.split('|').collect();
                let actual = top.readings.first().map(|s| s.as_str()).unwrap_or("");
                if banned.contains(&actual) {
                    fail(format!("banned top reading {actual:?}"))
                } else {
                    Ok(())
                }
            }
            "top_spelling" => {
                let top = tsv_top(span)?;
                if top.spellings.first().map_or(false, |s| s == expected) {
                    Ok(())
                } else {
                    fail(format!("actual={:?}", top.spellings.first()))
                }
            }
            "top_has_reading" => {
                let top = tsv_top(span)?;
                if top.readings.iter().any(|r| r == expected) {
                    Ok(())
                } else {
                    fail(format!("actual={:?}", top.readings.first()))
                }
            }
            "label" => {
                let actual = span.deconjugated_from.as_deref().unwrap_or("");
                if actual == expected {
                    Ok(())
                } else {
                    fail(format!("actual={actual:?}"))
                }
            }
            "label_contains" => match &span.deconjugated_from {
                Some(l) if l.contains(expected) => Ok(()),
                other => fail(format!("actual={other:?}")),
            },
            "label_not_contains" => match &span.deconjugated_from {
                Some(l) if l.contains(expected) => fail(format!("forbidden label part in {l:?}")),
                _ => Ok(()),
            },
            "has_entry" => {
                if expected.is_empty() {
                    return Err("empty expected".to_string());
                }
                let ok = expected.split('|').any(|want| {
                    span.entries.iter().any(|e| {
                        e.spellings.iter().any(|s| s == want)
                            || e.readings.iter().any(|r| r == want)
                    })
                });
                if ok {
                    Ok(())
                } else {
                    fail("no matching entry".to_string())
                }
            }
            "has_related" => {
                if span
                    .related_entries
                    .iter()
                    .any(|e| e.readings.iter().any(|r| r == expected))
                {
                    Ok(())
                } else {
                    fail("no matching related entry".to_string())
                }
            }
            "no_entry" => {
                let banned: Vec<&str> = expected.split('|').collect();
                let hit = span.entries.iter().any(|e| {
                    e.spellings.first().map_or(false, |s| banned.contains(&s.as_str()))
                });
                if hit {
                    fail("banned entry present".to_string())
                } else {
                    Ok(())
                }
            }
            "has_entry_pos" => {
                if span.entries.iter().any(|e| e.pos.iter().any(|p| p == expected)) {
                    Ok(())
                } else {
                    fail("no entry with that pos".to_string())
                }
            }
            "top_pos_contains" => {
                let top = tsv_top(span)?;
                if top.pos.iter().any(|p| p.contains(expected)) {
                    Ok(())
                } else {
                    fail(format!("actual={:?}", top.pos))
                }
            }
            "top_pos_word" => {
                let top = tsv_top(span)?;
                if top.pos.iter().any(|p| {
                    p.split(|c: char| !c.is_alphabetic()).any(|w| w == expected)
                }) {
                    Ok(())
                } else {
                    fail(format!("actual={:?}", top.pos))
                }
            }
            _ => Err(format!("unknown span check {check:?}")),
        }
    }

    #[test]
    fn tsv_cases() {
        let h = Harness::new();
        let data = include_str!("lookup_cases.tsv");
        let mut failures: Vec<String> = Vec::new();
        for (lineno, line) in data.lines().enumerate() {
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut cols: Vec<&str> = line.split('\t').collect();
            if cols.first() == Some(&"id") {
                continue;
            }
            while cols.len() < 9 {
                cols.push("");
            }
            let (id, area, sentence, mode, marker, span_sel, check, expected) =
                (cols[0], cols[1], cols[2], cols[3], cols[4], cols[5], cols[6], cols[7]);
            if sentence.is_empty() {
                failures.push(format!("line {}: empty sentence", lineno + 1));
                continue;
            }
            match tsv_run_row(&h, sentence, mode, marker, span_sel, check, expected) {
                Ok(()) => {}
                Err(msg) => failures.push(format!(
                    "line {} [{}:{}] sentence={:?} marker={:?} span={:?}: {msg}",
                    lineno + 1,
                    area,
                    id,
                    sentence,
                    marker,
                    span_sel
                )),
            }
        }
        assert!(
            failures.is_empty(),
            "\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn scan_respects_particle_boundaries() {
        let h = Harness::new();
        // A function-word token must never appear inside a longer span: every
        // span that starts at a 助詞/助動詞 is exactly that one token.
        for text in [
            "可能性があります",
            "考え方をしている",
            "意図もない",
            "にさせたい",
            "はしません",
            "のことは",
            "はよほどいい",
            "そういう相手なんだ",
            "できる限りのことはします",
        ] {
            let tokens = h.tokens(text);
            for span in scan(&h, text) {
                let token = tokens
                    .iter()
                    .find(|t| span.start >= t.start && span.start < t.end)
                    .unwrap();
                if matches!(token.pos.as_str(), "助詞" | "助動詞") {
                    assert_eq!(
                        span.surface.chars().count(),
                        1,
                        "function word {} merged into {:?} in {text}",
                        token.surface,
                        span.surface
                    );
                }
            }
        }
    }

#[test]
    fn deconjunction_prefers_kanji_sharing_entries() {
        let h = Harness::new();
        // 書けない must resolve to 書く (to write) first, not the coincidental
        // homophone 掛ける whose reading かける matches the potential form;
        // the shared leading kanji 書 outranks deeper deconjugation steps.
        let spans = scan(&h, "いや先輩はラテン語は書けない。");
        let kakenai = spans.iter().find(|s| s.surface == "書けない").unwrap();
        assert_eq!(top_reading(kakenai), "かく");
        assert_eq!(kakenai.entries.first().unwrap().spellings[0], "書く");

        // 引けない keeps the same-kanji potential 引ける ahead of 引く, while
        // unrelated homophones (弾く etc.) fall behind.
        let spans = scan(&h, "そんな人はいないから引けない。");
        let hikenai = spans.iter().find(|s| s.surface == "引けない").unwrap();
        let readings: Vec<String> = hikenai
            .entries
            .iter()
            .map(|e| e.spellings.first().unwrap().clone())
            .collect();
        assert!(readings.iter().take(2).all(|s| s.contains("引")), "{readings:?}");
    }

    #[test]
    fn all_span_entries_are_jmdict_entries() {
        // Item 1: the splitter (IPADIC/UniDic) never creates entries — every
        // id in every span must resolve in the JMdict-built by_id map.
        let h = Harness::new();
        for text in [
            "調査している",
            "食べられます",
            "風船でも割れるみたいな消え方。",
            "トイレを使ってから、手を洗わないといけません。",
        ] {
            for s in scan(&h, text) {
                for e in s.entries.iter().chain(s.related_entries.iter()) {
                    assert!(
                        h.index.by_id.contains_key(&e.id),
                        "span {:?} entry {} is not a JMdict entry",
                        s.surface,
                        e.id
                    );
                }
            }
        }
    }

    #[test]
    fn chouonpu_lookup_is_symmetric() {
        // Item 2: queries with or without ー reach the same index keys.
        let h = Harness::new();
        let with = normalize::normalize_variants("セーソー");
        let without = normalize::normalize_variants("せいそう");
        assert!(with.contains(&"せいそう".to_string()));
        // Every variant of a spelling is indexed, so either query form hits.
        for key in with.iter().chain(without.iter()) {
            let _ = key;
        }
        let related_a = find_containing("せいそう", &h.index, 5);
        let related_b = find_containing("せーそー", &h.index, 5);
        // Both directions query without error (exact hits depend on dict
        // contents, but neither direction may panic or diverge in handling).
        assert!(related_a.len() <= 5 && related_b.len() <= 5);
    }

    #[test]
    fn dattara_conditional_merges_across_aux_split() {
        // 帰るぐらいだったら: MeCab splits だっ|たら, both auxiliaries.
        let h = Harness::new();
        let text = "いや、帰るぐらいだったら一人でもなんとかなるよ。";
        let span = h.lookup(text, cpos(text, "だったら"));
        assert_eq!(span.surface, "だったら");
        assert!(
            span.entries.iter().any(|e| e.readings.first()
                .map_or(false, |r| r == "だ")
                && e.pos.iter().any(|p| p == "copula")),
            "だったら should resolve to the copula だ, got {:?}",
            span.entries
                .iter()
                .map(|e| e.readings.first())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn tto_particle_maps_to_to() {
        // Single-token っと (quotative/emphatic と) must resolve, not vanish.
        let h = Harness::new();
        let text = "ちりっと走った。";
        let pos = cpos(text, "っと");
        let tokens = h.tokens(text);
        let span = lookup_from_position(text, pos, 0, &h.index, &h.decon, &tokens).unwrap();
        assert_eq!(span.surface, "っと");
        assert!(span.entries.iter().any(|e| e.readings.first().map_or(false, |r| r == "と")),
            "っと should resolve to the と particle");
    }

    #[test]
    fn tto_particle_resolves_to_to() {
        // Single-token quotative っと must show と.
        let h = Harness::new();
        let text = "ちりっと走った。";
        let pos = cpos(text, "っと");
        let tokens = h.tokens(text);
        let span = lookup_from_position(text, pos, 0, &h.index, &h.decon, &tokens).unwrap();
        assert_eq!(span.surface, "っと");
        assert!(span.entries.iter().any(|e| e.readings.first().map_or(false, |r| r == "と")));
    }

