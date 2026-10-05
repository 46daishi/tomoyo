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

    #[test]
    fn copula_na_shortens_nonverb_stems() {
        let h = Harness::new();
        // たよりなんだ splits (option a): たよりな must not resolve to
        // 頼りない via the "imperative negative" な rule — たより is a
        // noun, so the な is the copula rentaikei.
        let span = h.lookup("君だけがたよりなんだ", 4);
        assert_eq!(span.surface, "たより");
        assert_eq!(top_reading(&span), "たより");
        // Plain たよりない surfaces still reach 頼りない (literal wins).
        let span = h.lookup("たよりない", 0);
        assert_eq!(span.surface, "たよりない");
        assert_eq!(top_reading(&span), "たよりない");
        // Verb stems keep the imperative reading.
        let span = h.lookup("食べな", 0);
        assert_eq!(span.surface, "食べな");
        assert_eq!(top_reading(&span), "たべる");
        // そうな has no entry: it falls back to そう (correct adnominal
        // behavior), while literal ような keeps 様な.
        let span = h.lookup("そうな顔", 0);
        assert_eq!(span.surface, "そう");
        let span = h.lookup("ような顔", 0);
        assert_eq!(span.surface, "ような");
    }

    #[test]
    fn keddo_sentence_spans() {
        let h = Harness::new();
        // けどぉ: けど conj joins via the completion pair, ぉ stays a
        // separate trailing syllable.
        let s = "えー、あたしは別になんともないけどぉ。";
        let span = h.lookup(s, 15);
        assert_eq!(span.surface, "けど");
        assert_eq!(top_reading(&span), "けど");
        let span = h.lookup(s, 17);
        assert_eq!(span.surface, "ぉ");
    }

    #[test]
    fn shiyagatta_suru_aux_binds() {
        let h = Harness::new();
        // バカにしやがった = する + the vulgar aux やがる; the literal
        // strip must restore the dictionary compound 馬鹿にする whole.
        let s = "てめっ! バカにしやがったな!?";
        let span = h.lookup(s, 5);
        assert_eq!(span.surface, "バカにしやがった");
        assert_eq!(top_reading(&span), "ばかにする");
        // The stance-picker predicates (怖がる/恥ずかしがる/楽しがる) still
        // hold together via lemmatization, not the aux rule.
        let span = h.lookup("こわがる", 0);
        assert_eq!(span.surface, "こわがる");
    }

    #[test]
    fn slurred_negative_verb_resolves_but_lookalikes_stay_gated() {
        let h = Harness::new();
        // なめんな (slurred negative) is a real verb lemma via deconjugation.
        let s = "農作業なめんなよ";
        let span = h.lookup(s, 3);
        assert_eq!(span.surface, "なめんな");
        assert_eq!(top_reading(&span), "なめる");
        // 好きな/へんな/静かな are adjective-lookalikes: the slurred step
        // is what makes the verb chain pass (好き→好く would need a
        // negative→slurred chain; the gate rejects single-step lookalikes).
        for (text, pos) in [("それ好きなんだ", 3), ("変な話", 0)] {
            let span = h.lookup(text, pos);
            assert!(
                !["かえる", "すく", "へる"].contains(&top_reading(&span).as_str()),
                "{text} must not resolve the slurred-verb lemma, got {}",
                top_reading(&span)
            );
        }
    }

    #[test]
    fn unknown_token_particle_partition() {
        let h = Harness::new();
        // カズちゃんとらぶらぶ: the と particle must not be absorbed into
        // a homophone (虎) inside the fragmented unknown run ゃんとらぶらぶ.
        let s = "カズちゃんとらぶらぶ~♪";
        let span = h.lookup(s, 5);
        assert_eq!(span.surface, "と");
        let span = h.lookup(s, 6);
        assert_eq!(span.surface, "らぶらぶ");
        assert_eq!(top_reading(&span), "ラブラブ");
        // Known tokens that cover a particle-looking kana stay whole.
        let span = h.lookup("かばん", 0);
        assert_eq!(span.surface, "かばん");
        assert_eq!(top_reading(&span), "かばん");
    }

    #[test]
    fn adverb_topic_wa_splits() {
        let h = Harness::new();
        // そうは must split into そう + は: the merged surface only matches
        // coincidental reading homophones (走破/争覇), while the real
        // adverb+は words (まずは/または) are single tokens.
        let span = h.lookup("そうは言われてもなぁ", 0);
        assert_eq!(span.surface, "そう");
        assert_eq!(top_reading(&span), "そう");
        let span = h.lookup("そうは言われてもなぁ", 2);
        assert_eq!(span.surface, "は");
        // Particles that form real words keep merging.
        let span = h.lookup("なぜか", 0);
        assert_eq!(span.surface, "なぜか");
        assert_eq!(top_reading(&span), "なぜか");
        let span = h.lookup("まずは", 0);
        assert_eq!(span.surface, "まずは");
        assert_eq!(top_reading(&span), "まずは");
    }

    #[test]
    fn obscure_margin_splits_only_on_frequency_gap() {
        let h = Harness::new();
        // あると -> ある + と (アルト 850 vs 有る 950).
        let span = h.lookup("迫力があると", 3);
        assert_eq!(span.surface, "ある");
        assert_eq!(top_reading(&span), "ある");
        // さんが -> さん + が (山河 670 vs 三 990).
        let span = h.lookup("月見山さんがいる", 3);
        assert_eq!(span.surface, "さん");
        assert_eq!(top_reading(&span), "さん");
        // Frequency ties stay whole.
        let span = h.lookup("どうか", 0);
        assert_eq!(span.surface, "どうか");
        let span = h.lookup("そうか", 0);
        assert_eq!(span.surface, "そうか");
    }

    #[test]
    fn suguni_keeps_same_lemma_adverbial() {
        let h = Harness::new();
        // すぐに stays whole: 直ぐに is 直ぐ + に of the same adverbial
        // family (keiyodoshi vs adverb must count as equal).
        let span = h.lookup("すぐにどっか。", 0);
        assert_eq!(span.surface, "すぐに");
        assert_eq!(top_reading(&span), "すぐに");
    }

    #[test]
    fn chan_suffix_completion_splits() {
        let h = Harness::new();
        // カズちゃんとらぶらぶ shreds as ち|ゃんとらぶらぶ: the closed
        // suffix list still reaches ちゃん from the ち cursor.
        let span = h.lookup("カズちゃんとらぶらぶ", 2);
        assert_eq!(span.surface, "ちゃん");
        assert!(
            span.entries.iter().any(|e| e.pos.iter().any(|p| p == "suffix")),
            "ちゃん should resolve as a suffix"
        );
        let span = h.lookup("カズちゃんとらぶらぶ", 5);
        assert_eq!(span.surface, "と");
        let span = h.lookup("カズちゃんとらぶらぶ", 6);
        assert_eq!(span.surface, "らぶらぶ");
        assert_eq!(top_reading(&span), "ラブラブ");
    }

    #[test]
    fn namenna_shredding_resolves() {
        let h = Harness::new();
        // 農作業なめんなよ shreds なめんな as な|めん|な: the completion
        // forms the span and the slurred-negative chain resolves it.
        let span = h.lookup("農作業なめんなよ", 3);
        assert_eq!(span.surface, "なめんな");
        assert_eq!(top_reading(&span), "なめる");
    }

    #[test]
    fn tunde_shredded_verb_resolves() {
        let h = Harness::new();
        // 連んでんだ shreds as 連|ん|でん: the te-form continuation end
        // plus the single-kanji gate exemption reach 連む (hang out with).
        // The slurred-negative exemption lets the full 連んでん resolve.
        let span = h.lookup("お前ら、相変わらず連んでんだな。", 9);
        assert_eq!(span.surface, "連んでん");
        assert_eq!(top_reading(&span), "つるむ");
    }

    #[test]
    fn single_kana_prefers_particle() {
        let h = Harness::new();
        // Single-kana hovers are almost always their particle reading:
        // 十/葉/絵-type nouns must not outrank the particle.
        for surface in [
            "は", "が", "を", "に", "の", "も", "で", "へ", "や", "か", "よ", "ね",
        ] {
            let span = h.lookup(surface, 0);
            assert_eq!(span.surface, surface);
            assert!(
                span.entries[0]
                    .pos
                    .iter()
                    .any(|p| p.split(|c: char| !c.is_alphabetic()).any(|w| w == "particle")),
                "{surface} should top a particle entry, got {:?}",
                span.entries[0].pos
            );
        }
    }

    #[test]
    fn trailing_sokuon_prefers_stem_or_falls_back() {
        let h = Harness::new();
        // いいよっ must not resolve to 言い寄る: strip the emphatic っ,
        // landing on the literal いいよ ("okay!") expression.
        let span = h.lookup("いいよっ", 0);
        assert_eq!(span.surface, "いいよ");
        assert_eq!(top_reading(&span), "いいよ");
        // Literal っ-final words are untouched.
        let span = h.lookup("あっ", 0);
        assert_eq!(span.surface, "あっ");
        assert_eq!(top_reading(&span), "あっ");
    }

    #[test]
    fn adverb_adjective_headword_stays_one_span() {
        let h = Harness::new();
        // なんともないけど -> なんともない + けど: the adverb+adjective
        // unit is a real headword.
        let spans = scan(&h, "なんともないけど");
        let surfaces: Vec<&str> = spans.iter().map(|s| s.surface.as_str()).collect();
        assert_eq!(surfaces, vec!["なんともない", "けど"]);
        let span = h.lookup("なんともないけど", 0);
        assert_eq!(span.surface, "なんともない");
        assert_eq!(top_reading(&span), "なんともない");
        // Non-headword adverb pairs still split.
        let span = h.lookup("とても親切", 0);
        assert_eq!(span.surface, "とても");
        let span = h.lookup("よく書く", 0);
        assert_eq!(span.surface, "よく");
    }

    #[test]
    fn kana_surface_prefers_kana_usual_entry() {
        let h = Harness::new();
        // せい (as in せいで, "due to") is usually kana: 所為 outranks
        // 性/制/生 when the surface is kana-only.
        let span = h.lookup("長身なせいで尚更迫力があると言うか", 3);
        assert_eq!(span.surface, "せい");
        assert_eq!(span.entries[0].spellings, vec!["所為".to_string()]);
    }

    #[test]
    fn copula_contaminated_stems_need_spelling() {
        let h = Harness::new();
        // 日だっけ -> 日 (not 襞): the っけ-recall stem 日だ only matches
        // coincidental readings.
        let span = h.lookup("レッスンの日だっけ", 5);
        assert_eq!(span.surface, "日");
        // だ stays its own single token (function-locked, like particles).
        let span = h.lookup("日だっけ", 1);
        assert_eq!(span.surface, "だ");
        assert_eq!(top_reading(&span), "だ");
        // Common-word stems absorb the tail whole (まだ + recall).
        let span = h.lookup("まだっけ", 0);
        assert_eq!(span.surface, "まだっけ");
        assert_eq!(top_reading(&span), "まだ");
        // Common hearsay absorbs the tail too (そうだっけ -> そうだ;
        // top entry ranking between そうだ readings is out of scope).
        let span = h.lookup("そうだっけ", 0);
        assert_eq!(span.surface, "そうだ");
    }

    #[test]
    fn temee_forms_resolve_to_temae() {
        let h = Harness::new();
        // てめー / てめっ shred as て|め|ー and て|め|っ with て
        // function-locked; fixed-expression completions form the span.
        for (text, pos, surface) in
            [("悠てめー", 1, "てめー"), ("てめー", 0, "てめー"), ("てめっ", 0, "てめっ")]
        {
            let span = h.lookup(text, pos);
            assert_eq!(span.surface, surface, "{text}");
            assert_eq!(top_reading(&span), "てめえ", "{text}");
        }
    }

    #[test]
    fn ttya_maps_to_ha_and_shiyagatta_to_suru() {
        let h = Harness::new();
        // っちゃ (世話焼いてるっちゃ) is topical は with sokuon.
        let span = h.lookup("世話焼いてるっちゃ、そうかもな。", 6);
        assert_eq!(span.surface, "っちゃ");
        assert_eq!(top_reading(&span), "は");
        // しやがった (バカにしやがったな) is する + vulgar やがる.
        let span = h.lookup("バカにしやがったな", 3);
        assert_eq!(span.surface, "しやがった");
        assert_eq!(top_reading(&span), "する");
    }

    #[test]
    fn noun_compound_without_verb_does_not_absorb_verb() {
        let h = Harness::new();
        // そんなわけありません -> そんな | わけ | ありません: 訳あり is a
        // real entry but has no verb reading, so it must not swallow ある.
        let spans = scan(&h, "そんなわけありません");
        let surfaces: Vec<&str> = spans.iter().map(|s| s.surface.as_str()).collect();
        assert_eq!(surfaces, vec!["そんな", "わけ", "ありません"]);
        assert_eq!(top_reading(&spans[2]), "ある");
    }

    #[test]
    fn contraction_stem_prefers_complete_word() {
        let h = Harness::new();
        // そうしちゃい must resolve to そう, not 奏する: the win came
        // through the ちゃい contraction, but そう alone is the word.
        let span = h.lookup("普通はそうしちゃいそうだよね。", 3);
        assert_eq!(span.surface, "そう");
        assert_eq!(top_reading(&span), "そう");
    }

    #[test]
    fn shredded_sokuon_does_not_hijack_morphology() {
        let h = Harness::new();
        // ったく tokenizes as っ|たく; the lone っ is a fragment, never a
        // verb stem, so its く base must not promote 九 ahead of the ったく
        // interjection (with 全く still discoverable as related).
        let span = h.lookup("ったく......はいはい、穹にはかなわないよ。", 0);
        assert_eq!(span.surface, "ったく");
        assert_eq!(top_reading(&span), "ったく");
        assert!(
            span.related_entries.iter().any(|e| e.readings.iter().any(|r| r == "まったく")),
            "全く should stay discoverable, got {:?}",
            span.related_entries.iter().map(|e| &e.readings).collect::<Vec<_>>()
        );
    }

    #[test]
    fn interjection_cursor_prefers_interjection_entry() {
        let h = Harness::new();
        // はいはい tokenizes as interjection tokens; both 這い這い and the
        // "yeah yeah" interjection match identically (Reading, same context),
        // so the cursor's 感動詞 tag must break the tie — not JMdict order.
        let span = h.lookup("ったく......はいはい、穹にはかなわないよ。", 9);
        assert_eq!(span.surface, "はいはい");
        assert!(
            span.entries[0].pos.iter().any(|p| p.contains("interjection")),
            "first entry should be the yeah-yeah interjection, got {:?}",
            span.entries.iter().take(3).map(|e| (&e.spellings, &e.readings, &e.pos)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn shimasu_resolves_to_suru_not_shiru() {
        let h = Harness::new();
        let span = h.lookup("連絡します", 2); // position of し
        assert_eq!(span.surface, "します");
        assert_eq!(top_reading(&span), "する",
            "first entry should read する (suru), got {:?}",
            span.entries.iter().map(|e| &e.readings[0]).collect::<Vec<_>>());
    }

    #[test]
    fn mae_context_orders_mae_before_zen() {
        let h = Harness::new();
        // 前 in 前にある is read まえ; 前(ぜん) is earlier in the dict, so
        // only the context reading can put 前(まえ) first.
        let span = h.lookup("前にある", 0);
        assert_eq!(span.surface, "前");
        assert_eq!(top_reading(&span), "まえ");
    }

    #[test]
    fn overlong_candidate_absorbs_auxiliaries_to_verb() {
        let h = Harness::new();
        // JL parity: 待ってみてください deconjugates through the ～てみる/～
        // ください auxiliaries all the way to 待つ (JL does the same), so the
        // whole phrase resolves instead of stopping at 待って.
        let span = h.lookup("待ってみてください", 0);
        assert_eq!(span.surface, "待ってみてください");
        assert_eq!(top_reading(&span), "まつ");
    }

    #[test]
    fn jl_style_auxiliary_resolution() {
        let h = Harness::new();
        // ～てくる: 入ってこない -> 入る.
        let span = h.lookup("入ってこない", 0);
        assert_eq!(span.surface, "入ってこない");
        assert_eq!(top_reading(&span), "はいる");

        // causative-passive: 知らされなかった -> 知る.
        let span = h.lookup("知らされなかった", 0);
        assert_eq!(span.surface, "知らされなかった");
        assert_eq!(top_reading(&span), "しる");

        // ～てしまう: 食べてしまった -> 食べる.
        let span = h.lookup("食べてしまった", 0);
        assert_eq!(top_reading(&span), "たべる");

        // stacked causative + passive + negative + past: 行かせられなかった -> 行く.
        let span = h.lookup("行かせられなかった", 0);
        assert_eq!(top_reading(&span), "いく");

        // contracted ～ちゃう: 言っちゃった -> 言う.
        let span = h.lookup("言っちゃった", 0);
        assert_eq!(top_reading(&span), "いう");
    }

    #[test]
    fn supplementary_must_and_copula_rules() {
        let h = Harness::new();
        // なければならない (JL's rules don't reduce this to the verb):
        // 食べなければならない -> 食べる.
        let span = h.lookup("食べなければならない", 0);
        assert_eq!(span.surface, "食べなければならない");
        assert_eq!(top_reading(&span), "たべる");

        // noun + だった -> the noun: 学生だった -> 学生.
        let span = h.lookup("学生だった", 0);
        assert_eq!(span.surface, "学生だった");
        assert_eq!(top_reading(&span), "がくせい");
    }

    #[test]
    fn taberareru_resolves_to_taberu() {
        let h = Harness::new();
        let span = h.lookup("食べられます", 0);
        assert_eq!(span.surface, "食べられます");
        assert_eq!(top_reading(&span), "たべる");
    }

    #[test]
    fn kana_kuru_negative_masu_still_resolves() {
        let h = Harness::new();
        // きませんでした: no kana deconjugation rule, but the き token's base
        // form (くる) lets morphology resolve it instead of く/きる guesses.
        let span = h.lookup("きませんでした", 0);
        assert_eq!(span.surface, "きませんでした");
        assert_eq!(top_reading(&span), "くる");
    }

    #[test]
    fn plain_kana_lookup_unaffected() {
        let h = Harness::new();
        // No verb morphology involved; should resolve via reading priority.
        let span = h.lookup("こんにちは", 0);
        assert_eq!(span.surface, "こんにちは");
        assert!(span.entries[0].readings[0] == "こんにちは");
    }

    #[test]
    fn particles_match_as_single_tokens() {
        let h = Harness::new();
        // が/を/も/に/は/の are dictionary entries (蛾, を, 藻, 二, 歯, 野)
        // and stay lookup-able, but only as their own token — never merged
        // into the next word.
        let cases = [
            ("可能性があります", 3, "が"),
            ("考え方をしている", 3, "を"),
            ("意図もない", 2, "も"),
            ("にさせたい", 0, "に"),
            ("はしません", 0, "は"),
            ("のことは", 0, "の"),
            ("はよほどいい", 0, "は"),
        ];
        for (text, pos, want) in cases {
            let span = h.lookup(text, pos);
            assert_eq!(span.surface, want, "particle at {pos} in {text}");
        }
    }

    #[test]
    fn auxiliaries_match_as_single_tokens() {
        let h = Harness::new();
        // なんだ segments as な/ん/だ — な (助動詞) is limited to its own
        // token, so なんだ never forms and 涙 (reading なんだ) is unreachable.
        let span = h.lookup("そういう相手なんだ", 6); // な
        assert_eq!(span.surface, "な");
        assert_ne!(top_reading(&span), "なみだ");

        let span = h.lookup("そういう相手なんだ", 8); // だ
        assert_eq!(span.surface, "だ");

        // ます as its own token still resolves (鱒/増す).
        let span = h.lookup("可能性があります", 6); // ます
        assert_eq!(span.surface, "ます");
    }

    #[test]
    fn verbs_after_particles_resolve_correctly() {
        let h = Harness::new();
        // はしません -> する (not 走る via ichidan ません->る).
        let span = h.lookup("はしません", 1); // し
        assert_eq!(span.surface, "しません");
        assert_eq!(top_reading(&span), "する");

        // にさせたい -> する (not にる via causative recursion).
        let span = h.lookup("にさせたい", 1); // さ
        assert_eq!(span.surface, "させたい");
        assert_eq!(top_reading(&span), "する");

        // できる限り -> 出来る限り (real phrase), not 模する from もできる.
        let span = h.lookup("できる限りのことはします", 0); // できる
        assert_eq!(span.surface, "できる限り");
        assert_eq!(top_reading(&span), "できるかぎり");

        // 意図もない -> ない is the adjective 無い, not 盛る.
        let span = h.lookup("意図もない", 3); // ない
        assert_eq!(span.surface, "ない");
        assert_eq!(top_reading(&span), "ない");

        // はよほど -> よほど is the adverb 余程, not 早よ.
        let span = h.lookup("はよほどいい", 1); // よほど
        assert_eq!(span.surface, "よほど");
        assert_eq!(top_reading(&span), "よほど");

        for text in ["食べてたんじゃなかった", "いいじゃないか"] {
            let spans = scan(&h, text);
            println!("  SENT {text}");
            for (i, s) in spans.iter().enumerate() {
                let entries: Vec<String> = s
                    .entries
                    .iter()
                    .take(3)
                    .map(|e| format!("{}({})", e.spellings.join("/"), e.readings.join("/")))
                    .collect();
                println!(
                    "  span[{i}] {:?}..{:?} {:?} decon={:?} entries={:?}",
                    s.start, s.end, s.surface, s.deconjugated_from, entries
                );
            }
        }
        for text in ["食べてたんじゃなかった", "いいじゃないか"] {
            let spans = scan(&h, text);
            println!("  SENT {text}");
            for (i, s) in spans.iter().enumerate() {
                let entries: Vec<String> = s
                    .entries
                    .iter()
                    .take(3)
                    .map(|e| format!("{}({})", e.spellings.join("/"), e.readings.join("/")))
                    .collect();
                println!(
                    "  span[{i}] {:?}..{:?} {:?} decon={:?} entries={:?}",
                    s.start, s.end, s.surface, s.deconjugated_from, entries
                );
            }
        }
        let span = h.lookup("のことは", 1); // こと
        assert_eq!(span.surface, "こと");
        assert_eq!(top_reading(&span), "こと");
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
    fn reading_deconj_beats_priority_for_homographs() {
        let h = Harness::new();
        // ひかれております is 惹かれる's te-form; JL ranks fewer deconj steps
        // before frequency, so 惹かれる (3 steps) beats 光る/引く (4 steps)
        // even though both are ichi1-frequency while 惹かれる is only spec1.
        let span = h.lookup("あなたに惹かれております", 4);
        assert_eq!(span.surface, "惹かれております");
        assert_eq!(top_reading(&span), "ひかれる");
    }

    #[test]
    fn mid_token_span_falls_back_to_surface_deconj() {
        let h = Harness::new();
        // The き of とき sits inside the single token とき (とく verb), so the
        // span きました has no honest token reading (its tokens read ました)
        // and must deconjugate the surface instead: きました -> 来る, not ます.
        let span = h.lookup("ときましたかぁ", 1);
        assert_eq!(span.surface, "きました");
        assert_eq!(top_reading(&span), "くる");
    }

    #[test]
    fn suru_contraction_beats_reading_homophones() {
        let h = Harness::new();
        // してん (する+て+ん) must resolve to する via the tokenizer's base
        // form, not to the してん reading-homophones 支店/視点.
        let span = h.lookup("顔してんの!", 1);
        assert_eq!(span.surface, "してん");
        assert_eq!(top_reading(&span), "する");
    }

    #[test]
    fn leading_and_trailing_punctuation_never_enters_span() {
        let h = Harness::new();
        // Leading dots: the cursor lands on "......" (an unknown 名詞 token),
        // but the span must snap forward to the actual word 苦労. 苦労 is a
        // suru-verb noun followed by している, so the whole economy resolves to
        // 苦労 (teiru) — exactly like 調査している -> 調査.
        let span = h.lookup("......苦労しているのも", 0);
        assert_eq!(span.surface, "苦労している");
        assert_eq!(span.start, 6);
        assert_eq!(top_reading(&span), "くろう");
        assert_eq!(span.deconjugated_from.as_deref(), Some("teiru"));

        // Trailing 記号: the span stops before 〜.
        let span = h.lookup("苦労〜", 0);
        assert_eq!(span.surface, "苦労");
        assert_eq!(top_reading(&span), "くろう");
    }

    #[test]
    fn unknown_tokens_do_not_extend_spans() {
        let h = Harness::new();
        // ぅ is an unknown token with an empty reading; without the break it
        // extended 疲れるぅ whose reading つかれる deconjugated to つく. The
        // span must stop at 疲れる so the literal spelling wins.
        let span = h.lookup("こっちの人格疲れるぅ〜......", 6);
        assert_eq!(span.surface, "疲れる");
        assert_eq!(top_reading(&span), "つかれる");
    }

    #[test]
    fn kire_resolves_to_kireru_in_context() {
        let h = Harness::new();
        // キレてる -> 切れる (the キレる slang), with 切る as the secondary
        // candidate rather than a coincidental deconj homophone outranking it.
        let span = h.lookup("キレてる", 0);
        assert_eq!(span.surface, "キレてる");
        assert_eq!(top_reading(&span), "きれる");
    }

    #[test]
    fn na_adjective_na_stays_with_the_noun() {
        let h = Harness::new();
        // The imperative な rule deconjugates 好きな -> 好く (すく), but a
        // verb-class result with no verb token in the span is coincidental.
        let span = h.lookup("好きな", 0);
        assert_eq!(span.surface, "好き");
        assert_eq!(top_reading(&span), "すき");
        let span = h.lookup("真面目な", 0);
        assert_eq!(span.surface, "真面目");
        assert_eq!(top_reading(&span), "まじめ");
    }

    #[test]
    fn deconj_labels_name_the_surface_conjugation() {
        let h = Harness::new();
        for (text, expected) in [
            ("した", "past"),
            ("高かった", "past"),
            ("面白くない", "negative"),
            ("呼んでいる", "teiru"),
            ("しなければならない", "must"),
            ("来てください", "polite request"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn nai_to_ikemasen_detects_must() {
        let h = Harness::new();
        for (text, base) in [
            ("洗わないといけません", "あらう"),
            ("洗わなければなりません", "あらう"),
            ("食べないといけない", "たべる"),
            ("食べなくてはいけません", "たべる"),
            ("しないといけません", "する"),
            ("しなければいけない", "する"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some("must"),
                "{text} should be labeled must, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn toire_sentence_spans() {
        let h = Harness::new();
        let spans = scan(&h, "トイレを使ってから、手を洗わないといけません。");
        let mut pairs: Vec<(String, String)> = spans
            .iter()
            .map(|s| (s.surface.clone(), s.deconjugated_from.clone().unwrap_or_default()))
            .collect();
        pairs.retain(|(s, _)| !s.is_empty());
        assert_eq!(
            pairs,
            vec![
                ("トイレ".to_string(), String::new()),
                ("を".to_string(), String::new()),
                ("使ってから".to_string(), "after doing".to_string()),
                ("手".to_string(), String::new()),
                ("を".to_string(), String::new()),
                ("洗わないといけません".to_string(), "must".to_string()),
            ]
        );
    }

    #[test]
    fn morph_labels_name_conjugations_for_kanji_bases() {
        let h = Harness::new();
        // The label comes from the deconjugation of the span reading, matched
        // to the base by dictionary entry (のむ == 飲む), never the bare base
        // form. から is a trailing particle, so 飲んでから still reaches the
        // te-form; 始めます dead-ends at the unrecordable ます-stem but is
        // still named by the first rule applied.
        for (text, base, expected) in [
            ("飲んで", "のむ", "te"),
            ("飲んでから", "のむ", "after doing"),
            ("食べてから", "たべる", "after doing"),
            ("飲んでいる", "のむ", "teiru"),
            ("食べています", "たべる", "polite + teiru"),
            ("始めます", "はじめる", "polite"),
            ("始めませんでした", "はじめる", "polite past negative"),
            ("言いませんでした", "いう", "polite past negative"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn combined_labels_name_all_meaningful_rules() {
        let h = Harness::new();
        // Labels combine every meaningful rule applied to the surface,
        // outermost first, skipping JL's intermediate-stem jargon — so
        // 住んでいた reads "past + teiru" rather than "past" and
        // 忘れてしまった reads "past + ended up" rather than "past".
        for (text, base, expected) in [
            ("住んでいた", "すむ", "past + teiru"),
            ("食べています", "たべる", "polite + teiru"),
            ("忘れてしまった", "わすれる", "past + ended up"),
            ("食べてしまった", "たべる", "past + ended up"),
            ("食べさせられてしまった", "たべる", "past + ended up + passive/potential + causative"),
            ("食べられます", "たべる", "polite + passive/potential"),
            ("しておきました", "する", "polite past + for now"),
            ("しておく", "する", "for now"),
            ("してくれました", "する", "polite past"),
            ("知らされなかった", "しる", "past + negative + causative passive"),
            ("言っちゃった", "いう", "past + ended up"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn suru_noun_phrases_resolve_to_the_noun() {
        let h = Harness::new();
        // 調査 + している is the suru verb 調査 doing teiru; the dictionary
        // headword is the noun 調査, not a literal 調査する or the coincidental
        // homophone 弔する (ちょうする) that whole-reading deconjugation finds.
        for (text, want_label) in [
            ("調査している", "teiru"),
            ("調査していた", "past + teiru"),
            ("調査して", "te"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(span.surface, text);
            assert_eq!(top_reading(&span), "ちょうさ", "{text} should resolve to 調査");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(want_label),
                "{text} should be labeled {want_label}, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn unknown_leading_token_deconjugates_surface_not_truncated_reading() {
        let h = Harness::new();
        // One-liner: ジイさんじゃない. The ジイ token is unknown (empty reading),
        // so the concatenated reading "さんじゃない" would misfire to さん. The
        // full surface じいさんじゃない deconjugates to じいさん (爺さん) instead.
        let span = h.lookup("ジイさんじゃない。", 0);
        assert_eq!(span.surface, "ジイさんじゃない");
        assert_eq!(top_reading(&span), "じいさん");
        assert_eq!(span.deconjugated_from.as_deref(), Some("copula"));

        // マズいんだ: the unknown katakana マズ must not collapse to the tail's
        // deconjugation (いんだ -> いぬ); instead the false maz/boundary is
        // crossed to form the real word マズい (= 不味い, まずい).
        let span = h.lookup("マズいんだ", 0);
        assert_eq!(span.surface, "マズい");
        assert_eq!(top_reading(&span), "まずい");
    }

    #[test]
    fn even_if_rules_name_the_condition() {
        let h = Harness::new();
        // JL has no rules for the も-ending te-form, so 急がなくても used to
        // fall back to naming an intermediate stem ("adverbial stem").
        for (text, base, expected) in [
            ("急がなくても", "いそぐ", "even if not"),
            ("食べなくても", "たべる", "even if not"),
            ("行かなくても", "いく", "even if not"),
            ("しなくても", "する", "even if not"),
            ("食べても", "たべる", "even if"),
            ("行っても", "いく", "even if"),
            ("飲んでも", "のむ", "even if"),
            ("高くても", "たかい", "even if"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn filler_token_allows_subtoken_span() {
        let h = Harness::new();
        // MeCab mis-tokenizes ともう as a single adjective token; the フィラー
        // cursor あ must be allowed to extend character-by-character so あと
        // still resolves instead of falling apart into あ/とも/う.
        let spans = scan(&h, "あともう一つ。");
        assert_eq!(spans[0].surface, "あと");
        assert_eq!(top_reading(&spans[0]), "あと");
        assert_eq!(spans[1].surface, "もう一つ");
        assert_eq!(top_reading(&spans[1]), "もうひとつ");
    }

    #[test]
    fn suru_noun_does_not_absorb_particle_phrase() {
        let h = Harness::new();
        // 除外するとして: として is an independent particle and must not extend
        // the suru-noun span — it used to collapse the whole thing to 除外.
        let spans = scan(&h, "除外するとして。");
        assert_eq!(spans[0].surface, "除外する");
        assert_eq!(top_reading(&spans[0]), "じょがい");
        assert_eq!(spans[1].surface, "として");
    }

    #[test]
    fn suru_noun_does_not_absorb_following_verb() {
        let h = Harness::new();
        // 徹底して伏せた: the independent verb 伏せた must not be absorbed into
        // the suru-noun span — it used to collapse to 徹底 entirely.
        let spans = scan(&h, "周りの大人が徹底して伏せた。");
        let hitoshi = spans.iter().find(|s| s.surface.contains("徹底")).unwrap();
        assert_eq!(hitoshi.surface, "徹底して");
        assert_eq!(top_reading(hitoshi), "てってい");
        let fuse = spans.iter().find(|s| s.surface.contains("伏せ")).unwrap();
        assert_eq!(fuse.surface, "伏せた");
        assert_eq!(top_reading(fuse), "ふせる");
    }

    #[test]
    fn verb_span_does_not_swallow_following_noun() {
        let h = Harness::new();
        // 引いたそう is 引く + the hearsay noun そう; the span must stop at
        // 引いた so 引く is found, instead of deconjugating the swallow 引いたそう
        // to ひる (干る).
        let spans = scan(&h, "すぐに引いたそうだ。");
        let hii = spans.iter().find(|s| s.surface.starts_with("引")).unwrap();
        assert_eq!(hii.surface, "引いた");
        assert_eq!(top_reading(hii), "ひく");
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
    fn explanatory_nda_never_merges_with_preceding_verb() {
        let h = Harness::new();
        // The ん of んだ is the explanatory copula, not a noun: したんだ must
        // split as した (past of する) + んだ, never resolve the whole
        // surface as a single rare verb (湑む したむ).
        let spans =
            scan(&h, "「もしもし。......お前、なにしたんだ? ジイさんバアさんたちパニック状態だぞ」");
        let shita = spans.iter().find(|s| s.surface == "した").unwrap();
        assert_eq!(top_reading(shita), "する");
        assert!(
            spans.iter().all(|s| s.surface != "したんだ"),
            "したんだ must not collapse into one span"
        );
        let nda = spans.iter().find(|s| s.surface == "んだ").unwrap();
        assert!(!nda.entries.is_empty(), "んだ should still be lookup-able");
    }

    #[test]
    fn explanatory_n_split_beats_homophone() {
        let h = Harness::new();
        // Stem + な + explanatory ん must not resolve to a coincidental
        // homophone of the whole run: どうなん -> 童男, いいん -> 委員,
        // 事なん -> 異なる (ことなん slurred to ことなる). The stem stands
        // alone; the copula keeps its own spans (だろう is formed from the
        // split だろ|う tokens).
        let spans = scan(&h, "奈緒ちゃんは、どうなんだろう?");
        let dou = spans.iter().find(|s| s.surface == "どう").unwrap();
        assert_eq!(top_reading(dou), "どう");
        assert!(
            spans.iter().all(|s| s.surface != "どうなん"),
            "どうなん must not resolve to 童男"
        );
        let darou = spans.iter().find(|s| s.surface == "だろう").unwrap();
        assert!(
            top_reading(darou).contains("だろ"),
            "だろう should read だろ, got {:?}",
            top_reading(darou)
        );

        let spans = scan(&h, "それがいいんじゃねーか!!");
        let ii = spans.iter().find(|s| s.surface == "いい").unwrap();
        assert_eq!(top_reading(ii), "いい");
        assert!(
            spans.iter().all(|s| s.surface != "いいん"),
            "いいん must not resolve to 委員"
        );

        let spans = scan(&h, "お昼借りてた弁当箱の事なんだけど...");
        let koto = spans.iter().find(|s| s.surface == "事").unwrap();
        assert_eq!(top_reading(koto), "こと");
        assert!(
            spans.iter().all(|s| s.surface != "事なん"),
            "事なん must not resolve to 異なる"
        );
    }

    #[test]
    fn honorific_prefix_and_small_vowel_coda_resolve() {
        let h = Harness::new();
        // The honorific お/ご prefix and a trailing small-vowel coda wrap a
        // word the tokenizer already split: resolve through the wrapper
        // instead of the coincidental homophone of the first half.
        let spans = scan(&h, "おまたせ。");
        let matase = spans.iter().find(|s| s.surface == "おまたせ").unwrap();
        assert_eq!(matase.entries.first().unwrap().spellings[0], "待つ");
        assert!(spans.iter().all(|s| s.surface != "おまた" && s.surface != "せ"));

        let spans = scan(
            &h,
            "夏休みが始まり、今年もおじいの家に遊びに来て何日ぐらいが過ぎただろう。",
        );
        let ojii = spans.iter().find(|s| s.surface == "おじい").unwrap();
        assert_eq!(ojii.entries.first().unwrap().spellings[0], "爺");
        assert!(spans.iter().all(|s| s.surface != "おじ"));

        // The ぁ of おばぁ spells the previous mora's vowel — it adds no
        // mora, so the word ends there, and the particle after おじい still
        // keeps its own span (the reduced じいと would resolve as 凝乎と).
        let spans = scan(
            &h,
            "おじいとおばぁが、近所に出かけてくると言ったので、僕は留守番を任されることになった。",
        );
        let obaa = spans.iter().find(|s| s.surface == "おばぁ").unwrap();
        assert_eq!(obaa.entries.first().unwrap().spellings[0], "祖母");
        let ojii = spans.iter().find(|s| s.surface == "おじい").unwrap();
        assert_eq!(ojii.entries.first().unwrap().spellings[0], "爺");
        assert!(
            spans.iter().all(|s| s.surface != "おじいと"),
            "おじい + と must not merge"
        );
        assert!(spans.iter().any(|s| s.surface == "と"), "と stays a span");
    }

    #[test]
    fn adverb_span_reaches_whole_headword() {
        let h = Harness::new();
        // どう + し + てる: the adverb continues through the whole headword
        // (どうしてる), not stopping at the する-stem where 同市 wins.
        let spans = scan(&h, "さて、穹のヤツどうしてるかな……");
        let span = spans
            .iter()
            .find(|s| s.surface == "どうしてる")
            .expect("どうしてる should span as one word");
        assert_eq!(top_reading(span), "どうしてる");
        assert!(
            spans.iter().all(|s| s.surface != "どうし"),
            "どうし must not resolve to 同市"
        );
        // An adverb + inflected-verb fragment still splits: そうほいほい
        // must not merge into そうほ (相補).
        let spans = scan(&h, "はは、そうほいほい面白い。");
        let sou = spans.iter().find(|s| s.surface == "そう").unwrap();
        assert_eq!(top_reading(sou), "そう");
        assert!(spans.iter().all(|s| s.surface != "そうほ"));
    }

    #[test]
    fn verb_reaches_one_kana_glued_into_next_noun() {
        let h = Harness::new();
        // 話し|と|くし: the く of 話しとく landed inside the noun token, so
        // the span ends one character in when that spelling resolves.
        let spans = scan(&h, "う、うん......それに、事情なら、私の方からも話しとくし......");
        let span = spans
            .iter()
            .find(|s| s.surface == "話しとく")
            .expect("話しとく should span to the verb's dictionary form");
        assert_eq!(span.entries.first().unwrap().spellings[0], "話す");
        assert!(spans.iter().all(|s| s.surface != "くし"));
    }

    #[test]
    fn yagatte_completion_forms_the_auxiliary() {
        let h = Harness::new();
        // 終わらせ|や|がって: the particle や and the がって token never
        // form the -yagaru auxiliary alone; the fixed pair completes it.
        let spans = scan(&h, "ちぇっ、普通に終わらせやがって。");
        let span = spans
            .iter()
            .find(|s| s.surface == "やがって")
            .expect("やがって should span as one auxiliary");
        assert!(
            top_reading(span).contains("がる"),
            "やがって should read やがる, got {:?}",
            top_reading(span)
        );
    }

    #[test]
    fn verb_split_explanatory_n_restores_the_phrase() {
        let h = Harness::new();
        // もう|そ|ん|な: the tokenizer reads もうそ as 申す and the run
        // slurred-deconjugates to もうそんな -> 申す, swallowing the real
        // phrase boundary. The verb must stop short of the explanatory ん so
        // もう + そんな can form.
        let spans = scan(&h, "あ、やばい......もうそんな時間かな......");
        assert!(
            spans.iter().all(|s| s.surface != "もうそんな"),
            "もうそんな must not resolve to 申す"
        );
        let mou = spans.iter().find(|s| s.surface == "もう").unwrap();
        assert_eq!(top_reading(mou), "もう");
        let sonna = spans.iter().find(|s| s.surface == "そんな").unwrap();
        assert_eq!(top_reading(sonna), "そんな");
        let jikan = spans.iter().find(|s| s.surface == "時間").unwrap();
        assert_eq!(top_reading(jikan), "じかん");
    }

    #[test]
    fn merged_teru_stays_within_suru_noun_phrase() {
        let h = Harness::new();
        // MeCab tokenizes 会議してる as 会議 + し + てる (て+いる merged into
        // one verb token), so the suru construction must still absorb the full
        // span and resolve to the noun 会議 rather than stopping at 会議し.
        let spans =
            scan(&h, "「いま団地の管理会社と、お寺で除霊するからどっちが費用を出すって会議してるよ」");
        let kaigi = spans.iter().find(|s| s.surface == "会議してる").unwrap();
        assert_eq!(top_reading(kaigi), "かいぎ");
        assert_eq!(kaigi.entries.first().unwrap().spellings[0], "会議");
    }

    #[test]
    fn past_aux_merged_tan_merges_with_copula_tail() {
        let h = Harness::new();
        // MeCab merges the past た with the explanatory ん into one 名詞 token
        // (たん): 寝てたんじゃなかったのか resolves as ONE span to 寝る, with
        // the copula tail named in the label — while lookups starting AT たん
        // itself still never collapse into たんじゃなかった -> 肝.
        let spans = scan(&h, "なんか用か。寝てたんじゃなかったのか");
        let whole = spans
            .iter()
            .find(|s| s.surface == "寝てたんじゃなかった")
            .unwrap();
        assert_eq!(top_reading(whole), "ねる");
        let label = whole.deconjugated_from.as_deref().unwrap();
        assert!(label.contains("explanatory"), "label names the tail, got {label:?}");
        // Skip-cycling still reaches the shorter 寝てた.
        let span = h.lookup("なんか用か。寝てたんじゃなかったのか", 6);
        assert_eq!(span.surface, "寝てたんじゃなかった");
        // A lookup starting at たん itself must not become 肝.
        let tan = h.lookup("なんか用か。寝てたんじゃなかったのか", 8);
        assert_ne!(tan.surface, "たんじゃなかった");
    }

    #[test]
    fn suru_noun_absorbs_passive_sareru() {
        let h = Harness::new();
        // 解除された is 解除 + される (passive): the suru construction must absorb
        // the passive れ (and the past た) and resolve to 解除, not stop at the
        // truncated 解除さ.
        let spans = scan(&h, "待てと言おうとしたが、容赦なく時間停止は解除された。");
        let kaijo = spans
            .iter()
            .find(|s| s.surface == "解除された")
            .unwrap();
        assert_eq!(top_reading(kaijo), "かいじょ");
        assert_eq!(kaijo.entries.first().unwrap().spellings[0], "解除");
        assert_eq!(kaijo.deconjugated_from.as_deref(), Some("past + passive"));
    }

    #[test]
    fn noun_particle_content_verb_split_into_three_spans() {
        let h = Harness::new();
        // 風船でも割れる is three phrases: the noun 風船, the inclusive particle
        // でも, and the verb 割れる. It must not deconjugate the whole string
        // into 諷する (ふうする).
        let spans = scan(&h, "風船でも割れるみたいな消え方。");
        assert!(
            spans.iter().all(|s| s.surface != "風船でも割れる"),
            "風船でも割れる must not collapse into one span"
        );
        let fusen = spans.iter().find(|s| s.surface == "風船").unwrap();
        assert_eq!(top_reading(fusen), "ふうせん");
        let demo = spans.iter().find(|s| s.surface == "でも").unwrap();
        assert!(!demo.entries.is_empty(), "でも should still be lookup-able");
        let wareru = spans.iter().find(|s| s.surface == "割れる").unwrap();
        assert_eq!(top_reading(wareru), "われる");
    }

    #[test]
    fn prohibitive_njanai_conjugates_to_the_verb() {
        let h = Harness::new();
        // 買うんじゃない (ん = の, じゃ = では) is the prohibitive "don't":
        // looking up 買う shows the whole tail as a conjugation of the verb.
        let spans = scan(&h, "野菜を買うんじゃない");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃない").unwrap();
        assert_eq!(top_reading(kau), "かう");
        assert_eq!(kau.entries.first().unwrap().spellings[0], "買う");
        assert_eq!(kau.deconjugated_from.as_deref(), Some("don't"));
        // The past んじゃなかった names the missed obligation "shouldn't have".
        let spans = scan(&h, "わざわざ買うんじゃなかった");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃなかった").unwrap();
        assert_eq!(top_reading(kau), "かう");
        assert_eq!(kau.entries.first().unwrap().spellings[0], "買う");
        assert_eq!(kau.deconjugated_from.as_deref(), Some("shouldn't have"));
    }

    #[test]
    fn prohibitive_njanai_works_across_verb_classes() {
        let h = Harness::new();
        // Godan (帰る), ichidan (食べる), suru (する), and kuru (来る) all
        // attach the prohibitive tail to their dictionary form.
        let spans = scan(&h, "そろそろ帰るんじゃない。");
        let kaeru = spans.iter().find(|s| s.surface == "帰るんじゃない").unwrap();
        assert_eq!(top_reading(kaeru), "かえる");
        assert_eq!(kaeru.deconjugated_from.as_deref(), Some("don't"));
        let spans = scan(&h, "食べるんじゃない");
        let taberu = spans.iter().find(|s| s.surface == "食べるんじゃない").unwrap();
        assert_eq!(top_reading(taberu), "たべる");
        assert_eq!(taberu.deconjugated_from.as_deref(), Some("don't"));
        // The suru of そうする merges with the adverb into one token, so the
        // prohibitive construction is verified through a standalone suru and
        // its んじゃなかった tail at a fresh span start instead.
        let spans = scan(&h, "まさか、するんじゃなかった。そろそろ帰るんじゃない。");
        let suru = spans.iter().find(|s| s.surface == "するんじゃなかった").unwrap();
        assert_eq!(suru.deconjugated_from.as_deref(), Some("shouldn't have"));
        assert_eq!(suru.entries.first().unwrap().spellings[0], "為る");
    }

    #[test]
    fn prolonging_kana_reading_split_across_unknown_token() {
        let h = Harness::new();
        // MeCab splits くしゃっと into くし + ゃっと (the latter an unknown
        // token). Such an unknown token normally stops a span, but here the
        // kana crossing including it is a real dictionary reading, so the
        // word must be found whole.
        let spans = scan(&h, "嬉しそうにくしゃっと顔をゆがめた。");
        let kushatto = spans.iter().find(|s| s.surface == "くしゃっと").unwrap();
        assert!(
            kushatto.entries.iter().any(|e| e.readings.contains(&"くしゃっと".to_string())),
            "くしゃっと should be found"
        );
        // A non-word crossing (疲れるぅ) must still not extend.
        let span = h.lookup("疲れるぅ", 0);
        assert_eq!(span.surface, "疲れる");
        assert_eq!(top_reading(&span), "つかれる");
    }

    #[test]
    fn referential_ko_split_prefers_parts_over_reading_entry() {
        let h = Harness::new();
        // もうこ (もう + こ) merges the referential こ into one token; when it
        // is followed by の (この), the parts もう + この must win over the
        // reading entry 蒙古, and both parts stay lookup-able.
        let spans = scan(&h, "俺はもうこの世界には――。");
        let mou = spans.iter().find(|s| s.surface == "もう").unwrap();
        assert!(mou.entries.iter().any(|e| e.spellings.contains(&"蒙".to_string())));
        let kono = spans.iter().find(|s| s.surface == "この").unwrap();
        assert!(kono.entries.iter().any(|e| e.spellings.contains(&"此の".to_string())));
        assert_eq!(spans.iter().find(|s| s.surface == "世界").unwrap().entries[0].spellings[0], "世界");
        assert!(spans.iter().all(|s| s.surface != "もうこ"), "蒙古 must not win");
        // Correlative ここ/そこ/どこ themselves never split.
        let span = h.lookup("ここ", 0);
        assert_eq!(span.surface, "ここ");
        let span = h.lookup("そこ", 0);
        assert_eq!(span.surface, "そこ");
    }

    #[test]
    fn suru_noun_potential_badable_forms_resolve_to_the_noun() {
        let h = Harness::new();
        // 交信できて (potential できる) directly continues the suru verb from
        // the noun 交信 — the construction must resolve to 交信, not collapse
        // into a coincidental reading (航する).
        // MeCab merges できて + ない into a single token (交信できてない), so
        // the suru continuation resolves 交信 as the head of that whole span —
        // the noun must surface, never collapse into 航する.
        let spans = scan(&h, "すぐ崩れるから、交信できてない");
        let koushin = spans.iter().find(|s| s.surface == "交信できてない").unwrap();
        assert_eq!(top_reading(koushin), "こうしん");
        assert_eq!(koushin.entries.first().unwrap().spellings[0], "交信");
        assert!(koushin.deconjugated_from.as_deref().unwrap().contains("potential"));
        // The bare potential (交信できて, no ない tail) continues the suru verb
        // all the way from the noun — it must resolve to 交信 as well.
        let spans = scan(&h, "すぐ崩れるから、交信できて");
        let koushin = spans.iter().find(|s| s.surface == "交信できて").unwrap();
        assert_eq!(top_reading(koushin), "こうしん");
        assert_eq!(koushin.entries.first().unwrap().spellings[0], "交信");
        assert!(koushin.deconjugated_from.as_deref().unwrap().contains("potential"));
    }

    #[test]
    fn nande_merges_across_copula_token_split() {
        let h = Harness::new();
        // After ...... MeCab tags な as a copula auxiliary (助動詞) + んで,
        // which would lock な to a single token — but なんで (何で, "why") is
        // a real adverb entry and must win whole...
        let spans = scan(&h, "30歳にもなって......なんで今泣いてるんだ。");
        let nande = spans.iter().find(|s| s.surface == "なんで").unwrap();
        assert!(nande.entries.iter().any(|e| e.spellings.contains(&"何で".to_string())));
        // ...while なんだ keeps な single (な+ん is never merged).
        let spans = scan(&h, "そういう相手なんだ");
        assert!(spans.iter().any(|s| s.surface == "な"));
        assert!(spans.iter().all(|s| s.surface != "なん"));
    }

    #[test]
    fn njanai_label_depends_on_stem_tense() {
        let h = Harness::new();
        // Dictionary-form stem + んじゃない/んじゃなかった is the prohibitive
        // ("don't" / "shouldn't have")...
        let spans = scan(&h, "野菜を買うんじゃない");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃない").unwrap();
        assert_eq!(kau.deconjugated_from.as_deref(), Some("don't"));
        let spans = scan(&h, "わざわざ買うんじゃなかった");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃなかった").unwrap();
        assert_eq!(kau.deconjugated_from.as_deref(), Some("shouldn't have"));
        // ...but past/negative stems carrying the same tail are explanatory —
        // 寝てたんじゃなかった is "wasn't sleeping", never "shouldn't have".
        let spans = scan(&h, "なんか用か。寝てたんじゃなかったのか");
        let nete = spans
            .iter()
            .find(|s| s.surface == "寝てたんじゃなかった")
            .unwrap();
        let label = nete.deconjugated_from.as_deref().unwrap();
        assert!(label.contains("explanatory"), "got {label:?}");
        assert!(!label.contains("shouldn't have"), "got {label:?}");
        let spans = scan(&h, "食べないんじゃない");
        let tabe = spans.iter().find(|s| s.surface == "食べないんじゃない").unwrap();
        assert_eq!(top_reading(tabe), "たべる");
        let label = tabe.deconjugated_from.as_deref().unwrap();
        assert!(label.contains("explanatory"), "got {label:?}");
    }

    #[test]
    fn suru_tari_construction_stays_one_span() {
        let h = Harness::new();
        // たり links suru clauses (保存したりしていた): the second し is the
        // suru verb continuing, not a new phrase — the whole span resolves to
        // 保存, never cut at 保存し.
        let spans = scan(
            &h,
            "どこかの掲示板を読んだり、流行の服の画像をいろいろ保存したりしていた。",
        );
        let hozon = spans.iter().find(|s| s.surface == "保存したりしていた").unwrap();
        assert_eq!(top_reading(hozon), "ほぞん");
        assert_eq!(hozon.entries.first().unwrap().spellings[0], "保存");
        // The たり listing is named instead of collapsing to bare "suru".
        assert_eq!(
            hozon.deconjugated_from.as_deref(),
            Some("past + teiru + tari")
        );
        assert!(spans.iter().all(|s| s.surface != "保存し"));
    }

    #[test]
    fn bare_datta_desita_resolve_to_copula() {
        let h = Harness::new();
        // だった after a particle has no noun stem to absorb into (穹 is a
        // name, から intervenes): it resolves to the copula だ itself, past.
        let spans = scan(&h, "その着信とメールは全て穹からだった。");
        let datta = spans.iter().find(|s| s.surface == "だった").unwrap();
        assert_eq!(top_reading(datta), "だ");
        assert_eq!(datta.deconjugated_from.as_deref(), Some("past"));
        // Same for polite でした after a particle (JL's rewrite rules map
        // でした -> です, labeled "past"; the entry itself carries politeness).
        let spans = scan(&h, "彼からでした。");
        let deshita = spans.iter().find(|s| s.surface == "でした").unwrap();
        assert_eq!(top_reading(deshita), "です");
        assert_eq!(deshita.deconjugated_from.as_deref(), Some("past"));
    }

    #[test]
    fn copula_tari_listing_stays_whole() {
        let h = Harness::new();
        // だったり/でしたり have no tari rule of their own — JL routes them
        // through the godan-verb tari rules (だったる/だつ), which the
        // verb-class gate rejects because a copula span carries no verb
        // token. The span used to fall back to だった plus a stray り (利).
        let spans = scan(&h, "放課後だったり、朝だったりにやる。");
        let dattari = spans
            .iter()
            .find(|s| s.surface == "放課後だったり")
            .unwrap();
        assert_eq!(top_reading(dattari), "ほうかご");
        assert_eq!(dattari.deconjugated_from.as_deref(), Some("copula + tari"));
        let asadattari = spans.iter().find(|s| s.surface == "朝だったり").unwrap();
        assert_eq!(top_reading(asadattari), "あさ");
        assert!(spans.iter().all(|s| s.surface != "り"));
        // After a particle there is no stem to absorb it, so the bare
        // listing form resolves to the copula itself, past.
        let spans = scan(&h, "彼からだったり、彼女からだったりする。");
        let bare = spans.iter().find(|s| s.surface == "だったり").unwrap();
        assert_eq!(top_reading(bare), "だ");
        assert_eq!(bare.deconjugated_from.as_deref(), Some("past + tari"));
        let spans = scan(&h, "彼からでしたり、彼女からでしたりする。");
        let bare = spans.iter().find(|s| s.surface == "でしたり").unwrap();
        assert_eq!(top_reading(bare), "です");
        assert_eq!(bare.deconjugated_from.as_deref(), Some("past + tari"));
    }

    #[test]
    fn tate_suffix_names_freshly_finished() {
        let h = Harness::new();
        // したて is し (する) + the freshly-finished suffix たて, not the
        // homophone 下手. The span stays together and the tag names it.
        let spans = scan(&h, "転校したては、みんな孤独なんだ。");
        let shitate = spans.iter().find(|s| s.surface == "したて").unwrap();
        assert_eq!(top_reading(shitate), "する");
        assert_eq!(
            shitate.deconjugated_from.as_deref(),
            Some("right after doing")
        );
    }

    #[test]
    fn hane_splits_to_ha_plus_ne() {
        let h = Harness::new();
        // はね is は + ね, not the coincidental reading 羽.
        let spans = scan(
            &h,
            "うちの部はね、このシーズンまで自主トレだったり、休みだったりで、泳ぐことはないの。",
        );
        assert!(spans.iter().any(|s| s.surface == "は"));
        assert!(spans.iter().any(|s| s.surface == "ね"));
        assert!(spans.iter().all(|s| s.surface != "はね"));
    }

    #[test]
    fn fused_particle_split_keeps_real_stems() {
        let h = Harness::new();
        // A real verb stem before punctuation or an auxiliary is not a fused
        // particle: 跳ね、かねない keep their stems, never は/か + ね.
        let spans = scan(&h, "うさぎが跳ね、止まった。");
        assert!(spans.iter().any(|s| s.surface == "跳ね"));
        assert!(spans.iter().all(|s| s.surface != "は"));
        let spans = scan(&h, "お金は兼ねないよ。");
        assert!(spans.iter().any(|s| s.surface == "兼ねない"));
        assert!(spans.iter().all(|s| s.surface != "か"));
    }

    #[test]
    fn copula_absorption_lists_stem_readings() {
        let h = Harness::new();
        // 休みだったり names the stem (休み) but must still list the stem's
        // own readings (休む), exactly like the bare 休み span does.
        let spans = scan(
            &h,
            "うちの部はね、このシーズンまで自主トレだったり、休みだったりで、泳ぐことはないの。",
        );
        let yasumi = spans
            .iter()
            .find(|s| s.surface == "休みだったり")
            .unwrap();
        assert!(
            yasumi
                .entries
                .iter()
                .any(|e| e.spellings.iter().any(|s| s == "休み")),
            "stem 休み missing from 休みだったり entries",
        );
        assert!(
            yasumi
                .entries
                .iter()
                .any(|e| e.spellings.iter().any(|s| s == "休む")),
            "stem reading 休む missing from 休みだったり entries",
        );
    }

    #[test]
    fn nan_merges_across_continuing_da() {
        let h = Harness::new();
        // どうなんだろう: なん reads as 何 — while どう and だろう keep
        // their own spans and どうなん never forms (童男).
        let spans = scan(&h, "奈緒ちゃんは、どうなんだろう?");
        let nan = spans.iter().find(|s| s.surface == "なん").unwrap();
        assert!(
            nan.entries.iter().any(|e| e.spellings.iter().any(|s| s == "何")),
            "どうなんだろう's なん should read 何",
        );
        assert!(spans.iter().any(|s| s.surface == "どう"));
        assert!(spans.iter().any(|s| s.surface == "だろう"));
        assert!(spans.iter().all(|s| s.surface != "どうなん"));
        // 事なんだけど: なん + だ + けど, not な + んだ.
        let spans = scan(
            &h,
            "うん......突然ゴメンね...お昼借りてた弁当箱の事なんだけど...",
        );
        let nan = spans.iter().find(|s| s.surface == "なん").unwrap();
        assert!(
            nan.entries.iter().any(|e| e.spellings.iter().any(|s| s == "何")),
            "事なんだけど's なん should read 何",
        );
        assert!(spans.iter().all(|s| s.surface != "な"));
    }

    #[test]
    fn meshi_chouonpu_falls_back_to_meshi() {
        let h = Harness::new();
        // メシー is emphatic メシ (飯), not the orphan reading 盲.
        let spans = scan(&h, "よっしゃー! メシメシー!");
        let meshi = spans.iter().find(|s| s.surface == "メシー").unwrap();
        assert_eq!(meshi.entries[0].spellings[0], "飯");
    }

    #[test]
    fn ndarou_shortens_to_stem() {
        let h = Harness::new();
        // 良いんだろうか used to resolve whole to 余韻 (via よいん):
        // stem + explanatory ん + conjecture splits like the ん-split does.
        let spans = scan(&h, "確かめても良いんだろうか。");
        let yoi = spans.iter().find(|s| s.surface == "良い").unwrap();
        assert!(
            yoi.entries.iter().any(|e| e.spellings.iter().any(|s| s == "良い")),
            "良いんだろうか should shorten to 良い",
        );
        assert!(spans.iter().any(|s| s.surface == "だろう"));
        assert!(spans.iter().all(|s| s.surface != "良いんだろうか"));
        // いいんだろうけど did the same via 委員 + conjecture: the stem
        // wins, なん/だろう/けど keep their spans, and しかない (which
        // しか|ない tokenizes into) merges to the expression.
        let spans = scan(
            &h,
            "何か、もっと簡単にできたらいいんだろうけど、まぁ、慣れていくしかないだろう。",
        );
        let ii = spans.iter().find(|s| s.surface == "いい").unwrap();
        assert!(
            ii.entries.iter().any(|e| e.spellings.iter().any(|s| s == "良い")),
            "いいんだろうけど should shorten to いい",
        );
        let nan = spans.iter().find(|s| s.surface == "なん").unwrap();
        assert!(
            nan.entries.iter().any(|e| e.spellings.iter().any(|s| s == "何")),
            "いいんだろうけど's なん should read 何",
        );
        assert!(spans.iter().any(|s| s.surface == "だろう"));
        assert!(spans.iter().all(|s| s.surface != "いいんだろう"));
        let shikanai = spans.iter().find(|s| s.surface == "しかない").unwrap();
        assert_eq!(top_reading(shikanai), "しかない");
        assert!(spans.iter().all(|s| s.surface != "しか"));
    }

    #[test]
    fn doushitai_splits_suru_verb() {
        let h = Harness::new();
        // どうしたい is どう + したい (する + want-to), not the coincidental
        // noun どうし (動詞/同志).
        let spans = scan(&h, "そもそも確かめて僕はどうしたいのか。");
        assert!(spans.iter().any(|s| s.surface == "どう"));
        let shitai = spans.iter().find(|s| s.surface == "したい").unwrap();
        assert_eq!(top_reading(shitai), "する");
        assert!(spans.iter().all(|s| s.surface != "どうしたい"));
    }

    #[test]
    fn sanshoku_carves_counter() {
        let h = Harness::new();
        // 一日三食分 shreds as 三|食分: hovering 三 must reach 三食 (the
        // number is never the point), topping its own list.
        let spans = scan(
            &h,
            "弁当を作るとなると、朝、夜も含めて一日三食分の材料が必要になる。",
        );
        let sanshoku = spans.iter().find(|s| s.surface == "三食").unwrap();
        assert_eq!(sanshoku.entries[0].spellings[0], "三食");
    }

    #[test]
    fn dekitemo_labels_even_if() {
        let h = Harness::new();
        // 出来ても is 出来る + ても ("even if"), like 食べても — even though
        // MeCab mistags the stem as a noun, the ても-rule proves the verb.
        let spans = scan(
            &h,
            "電子レンジのトースターモードでパンを焼くことは出来ても、まだ炊飯器のセットすら出来ない。",
        );
        let dekitemo = spans.iter().find(|s| s.surface == "出来ても").unwrap();
        assert_eq!(top_reading(dekitemo), "できる");
        assert_eq!(dekitemo.deconjugated_from.as_deref(), Some("even if"));
    }

    #[test]
    fn chatchato_completion_reaches_adverb() {
        let h = Harness::new();
        // ちゃっちゃと ("quickly") shreds with a function-locked ちゃっ
        // head: without the merged end, ちゃう wins the list. The direct
        // adverb match must rank first.
        let spans = scan(&h, "そう? じゃあ後でいくね。よーし、買い物をちゃっちゃと済まさないと。");
        let chatchato = spans.iter().find(|s| s.surface == "ちゃっちゃと").unwrap();
        assert!(
            chatchato.entries[0]
                .readings
                .iter()
                .any(|r| r == "ちゃっちゃと"),
            "ちゃっちゃと should top its own adverb entry",
        );
    }

    #[test]
    fn omake_shitoita_splits() {
        let h = Harness::new();
        // おまけしといた is おまけ + しといた (しておいた): the suru path
        // must not absorb the bare し when a とく-form continues, and
        // しといた resolves to する like しとく does.
        let spans = scan(&h, "おまけしといたから、また来ておくれ!");
        let omake = spans.iter().find(|s| s.surface == "おまけ").unwrap();
        assert_eq!(omake.entries[0].spellings[0], "お負け");
        assert!(spans.iter().all(|s| s.surface != "おまけし"));
        let shitoita = spans.iter().find(|s| s.surface == "しといた").unwrap();
        assert_eq!(top_reading(shitoita), "する");
        assert_eq!(
            shitoita.deconjugated_from.as_deref(),
            Some("in advance (casual)")
        );
    }

    #[test]
    fn ari_continuative_prefers_aru() {
        let h = Harness::new();
        // あり (有る-continuative) must top 有る, not 蟻: the renyoukei rule
        // reaches ある and the morphological path trusts MeCab's verb.
        // Same sentence: だけど merges (だけ|ど) and 炒めちゃう stays whole
        // (炒める), never あり->蟻-first / だけ|ど-split / 炒め+ちゃう.
        let spans = scan(&h, "あとは、乱暴だけど全部まとめて炒めちゃうのもありね。");
        let ari = spans.iter().find(|s| s.surface == "あり").unwrap();
        assert_eq!(ari.entries[0].spellings[0], "有る");
        let dakedo = spans.iter().find(|s| s.surface == "だけど").unwrap();
        assert_eq!(top_reading(dakedo), "だけど");
        assert!(spans.iter().all(|s| s.surface != "だけ"));
        let itamechau = spans.iter().find(|s| s.surface == "炒めちゃう").unwrap();
        assert_eq!(itamechau.entries[0].spellings[0], "炒める");
    }

    #[test]
    fn omatasetsu_strips_sokuon() {
        let h = Harness::new();
        // お待たせっ is お + 待たせ (causative) + emphatic っ: the span
        // lands on お待たせ naming 待たせる, never お待た (お股).
        let spans = scan(&h, "お待たせっ、ゴメンね、遅くなって?");
        let omatase = spans.iter().find(|s| s.surface == "お待たせ").unwrap();
        assert!(
            omatase
                .entries
                .iter()
                .any(|e| e.spellings.iter().any(|s| s == "待たせる")),
            "お待たせっ should resolve to 待たせる",
        );
        assert!(
            omatase
                .deconjugated_from
                .as_deref()
                .map_or(false, |l| l.contains("honorific")),
            "お待たせっ should keep its honorific label",
        );
        assert!(spans.iter().all(|s| s.surface != "お待た"));
    }

    #[test]
    fn minitsukeru_compound_stays_whole() {
        let h = Harness::new();
        // 身につける is a dictionary compound: noun + に + verb-base being
        // a key exempts the verb end from the separator guards.
        let spans = scan(
            &h,
            "奈緒ちゃんは、エプロンを身につけると流しに材料を並べて、水洗いを始めた。",
        );
        let mi = spans.iter().find(|s| s.surface == "身につける").unwrap();
        assert!(
            mi.entries
                .iter()
                .any(|e| e.spellings.iter().any(|s| s == "身につける")),
            "身につける should resolve whole",
        );
    }

    #[test]
    fn kawa_wo_muku_compound_stays_whole() {
        let h = Harness::new();
        // 皮をむく, same shape with を (and an inflected 皮をむくと tail
        // still reaches it through deconjugation).
        let spans = scan(
            &h,
            "手早く皮をむくと、包丁で細切りにされていくジャガイモとタマネギ、にんじん。",
        );
        let kawa = spans.iter().find(|s| s.surface == "皮をむく").unwrap();
        assert!(
            kawa.entries
                .iter()
                .any(|e| e.spellings.iter().any(|s| s == "皮をむく" || s == "皮を剥く")),
            "皮をむく should resolve whole",
        );
    }

    #[test]
    fn must_construction_variants_stay_whole() {
        let h = Harness::new();
        // Polite past, colloquial stems, ねば, では, and kuru: the いけない/
        // ならない tail must not split into its own span.
        for (text, pos, surface, base) in [
            ("食べなければいけませんでした", 0, "食べなければいけませんでした", "たべる"),
            ("宿題をしなければいけない", 3, "しなければいけない", "する"),
            ("行かなきゃいけない", 0, "行かなきゃいけない", "いく"),
            ("しなくちゃいけない", 0, "しなくちゃいけない", "する"),
            ("来なければいけない", 0, "来なければいけない", "くる"),
            ("行かねばならない", 0, "行かねばならない", "いく"),
            ("食べなくっちゃいけない", 0, "食べなくっちゃいけない", "たべる"),
            ("行かないといけませんでした", 0, "行かないといけませんでした", "いく"),
        ] {
            let span = h.lookup(text, pos);
            assert_eq!(span.surface, surface, "{text} should stay one span");
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some("must"),
                "{text} should be labeled must, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn must_tail_does_not_swallow_following_verb() {
        let h = Harness::new();
        // A conditional followed by an unrelated verb is not a must
        // construction: なければ opens the chain, but 食べる is not a
        // must-auxiliary (いく/いける/なる), so it must still split.
        let spans = scan(&h, "行かなければ食べる。");
        assert!(spans.iter().all(|s| s.surface != "行かなければ食べる"));
        let nake = spans.iter().find(|s| s.surface == "行かなければ").unwrap();
        assert_eq!(top_reading(nake), "いく");
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
    fn volitional_hairu_resolves_despite_homograph() {
        let h = Harness::new();
        // 本題に入ろう: volitional of 入る — MeCab mistags 入ろう as a noun
        // (reading にゅうろう), but surface deconjugation still reaches 入る.
        // (JMdict splits 入る into いる/はいる entries sharing the spelling;
        // either headword is correct here.)
        let spans = scan(&h, "助かる。さっそく本題に入ろう。");
        let hairu = spans.iter().find(|s| s.surface == "入ろう").unwrap();
        assert_eq!(hairu.entries.first().unwrap().spellings[0], "入る");
        assert_eq!(hairu.deconjugated_from.as_deref(), Some("volitional"));
    }

    #[test]
    fn suru_noun_with_wo_particle_stays_one_span() {
        let h = Harness::new();
        // 質問をする is a JMdict expression headword, so 質問をしている is one
        // span resolving to 質問 (teiru) — not 質問 / を / している.
        let spans = scan(&h, "何度も同じ質問をしている。");
        let shitsumon = spans.iter().find(|s| s.surface == "質問をしている").unwrap();
        assert_eq!(top_reading(shitsumon), "しつもん");
        assert_eq!(shitsumon.deconjugated_from.as_deref(), Some("teiru"));
    }

    #[test]
    fn suru_noun_causative_stays_one_span() {
        let h = Harness::new();
        // 消失させる (causative of 消失する) resolves to 消失 as one span.
        let spans = scan(&h, "先輩を消失させるのは、俺だった。");
        let shoshitsu = spans.iter().find(|s| s.surface == "消失させる").unwrap();
        assert_eq!(top_reading(shoshitsu), "しょうしつ");
        // Bare conditional negation of the causative also stays whole.
        let spans = scan(&h, "儀式を完了させなければ");
        let kanryo = spans.iter().find(|s| s.surface == "完了させなければ").unwrap();
        assert_eq!(top_reading(kanryo), "かんりょう");
    }

    #[test]
    fn teiku_past_compound_stays_one_span() {
        let h = Harness::new();
        // 盗んでいった is 盗む + ていく (teiku) + past, one span (the
        // explanatory んです tail rides along via copula-tail stripping).
        let spans = scan(&h, "うちから大事な物を盗んでいったんです");
        let nusumu = spans.iter().find(|s| s.surface == "盗んでいったんです").unwrap();
        assert_eq!(top_reading(nusumu), "ぬすむ");
        assert_eq!(nusumu.entries.first().unwrap().spellings[0], "盗む");
    }

    #[test]
    fn nande_and_ima_split_correctly() {
        let h = Harness::new();
        // なんで is the single 何で (why) entry; 今 must not absorb the
        // following verb (今泣いてる -> 今 + 泣いてる, never 忌む).
        let spans = scan(&h, "なんで今泣いてるんだ。");
        let nande = spans.iter().find(|s| s.surface == "なんで").unwrap();
        assert!(nande.entries.iter().any(|e| e.spellings.contains(&"何で".to_string())));
        let ima = spans.iter().find(|s| s.surface == "今").unwrap();
        assert!(!ima.entries.is_empty());
        let naiteru = spans.iter().find(|s| s.surface == "泣いてる").unwrap();
        assert_eq!(top_reading(naiteru), "なく");
        assert!(
            spans.iter().all(|s| !s.surface.contains("忌")),
            "must not resolve to 忌む"
        );
    }

    #[test]
    fn kanji_surface_prefers_kanji_sharing_entry() {
        let h = Harness::new();
        // 引かれた is written with 引: 引く must beat 惹かれる even though
        // 惹かれる deconjugates in fewer steps.
        let spans = scan(&h, "ナタが引かれた。");
        let hikareta = spans.iter().find(|s| s.surface == "引かれた").unwrap();
        assert_eq!(hikareta.entries.first().unwrap().spellings[0], "引く");
    }

    #[test]
    fn stem_sugiru_compound_resolves_to_front_verb() {
        let h = Harness::new();
        // 取り過ぎた is 取る + すぎる ("too much" + past), not the noun 取り過ぎ.
        let spans = scan(&h, "だが俺はもう歳を取り過ぎた。");
        let torisugi = spans.iter().find(|s| s.surface == "取り過ぎた").unwrap();
        assert_eq!(torisugi.entries.first().unwrap().spellings[0], "取る");
        assert_eq!(
            torisugi.deconjugated_from.as_deref(),
            Some("past + too much")
        );
    }

    fn cpos(text: &str, sub: &str) -> usize {
        let b = text.find(sub).unwrap_or_else(|| panic!("{sub:?} not in {text:?}"));
        text[..b].chars().count()
    }

    #[test]
    fn kiku_and_tedasuke_regions_resolve() {
        // No backend bug here (reported as showing only する): every hover
        // position already resolves correctly — 聞いてる -> 聞く,
        // 手助けをしてくれる -> 手助け, verb-start してくれる -> する.
        let h = Harness::new();
        let text = "豹馬(聞いてるぞ。5人で手助けをしてくれるんだよな)";
        let span = h.lookup(text, cpos(text, "聞いて"));
        assert_eq!(span.surface, "聞いてる");
        assert_eq!(top_reading(&span), "きく");
        let span = h.lookup(text, cpos(text, "手助け"));
        assert_eq!(span.surface, "手助けをしてくれる");
        assert_eq!(top_reading(&span), "てだすけ");
    }

    #[test]
    fn adjective_causative_saseru_stays_one_span() {
        // 気を悪くさせて: the ku-stem adjective continues into させる.
        let h = Harness::new();
        let text = "気を悪くさせてごめんね、本当に。";
        let span = h.lookup(text, cpos(text, "悪く"));
        assert_eq!(span.surface, "悪くさせて");
        assert_eq!(top_reading(&span), "わるい");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("causative + te")
        );
    }

    #[test]
    fn suru_noun_absorbs_chau_contraction() {
        // 遅刻しちゃう must stay one span (遅刻), not cut at し.
        let h = Harness::new();
        let text = "っと、そろそろ私、行かなきゃ遅刻しちゃう。";
        let span = h.lookup(text, cpos(text, "遅刻"));
        assert_eq!(span.surface, "遅刻しちゃう");
        assert_eq!(top_reading(&span), "ちこく");
    }

    #[test]
    fn koto_ni_natta_resolves_to_koto_ni_naru() {
        // 通うことになった: formal-noun こと + なる across the に particle.
        let h = Harness::new();
        let text = "学校の方からも連絡があり、いよいよ今日から通うことになった。";
        let span = h.lookup(text, cpos(text, "ことにな"));
        assert_eq!(span.surface, "ことになった");
        assert_eq!(top_reading(&span), "ことになる");
        assert_eq!(span.deconjugated_from.as_deref(), Some("past"));
    }

    #[test]
    fn shredded_warui_shi_merges_okurigana() {
        // MeCab shreds 悪いし as 悪|いし: the single-hiragana literal merge
        // must still reach 悪い, not the 悪 prefix noun.
        let h = Harness::new();
        let text = "あ、だったら、立ちっぱなしも悪いし中に入って待っててよ。";
        let span = h.lookup(text, cpos(text, "悪いし"));
        assert_eq!(span.surface, "悪い");
        assert_eq!(top_reading(&span), "わるい");
    }

    #[test]
    fn suru_noun_absorbs_nagara() {
        // 案内しながら行く: ながら continues the suru-verb chain.
        let h = Harness::new();
        let text = "学校まではるちゃん達を案内しながら行くつもりだったから。";
        let span = h.lookup(text, cpos(text, "案内し"));
        assert_eq!(span.surface, "案内しながら");
        assert_eq!(top_reading(&span), "あんない");
        assert_eq!(span.deconjugated_from.as_deref(), Some("while"));
    }

    #[test]
    fn unknown_kanji_okurigana_verb_stays_one_span() {
        // 淹れてもらう tokenized as 淹|れ|て|もらう: the unknown kanji's
        // following verb is the okurigana continuation, resolving to 淹れる.
        let h = Harness::new();
        let text = "そんな、淹れてもらうのにわざわざ注文なんてつけないよ。";
        let span = h.lookup(text, cpos(text, "淹れて"));
        assert_eq!(span.surface, "淹れてもらう");
        assert_eq!(span.entries[0].spellings[0], "淹れる");
        assert_eq!(top_reading(&span), "いれる");
    }

    #[test]
    fn suru_noun_absorbs_chau_contraction_imperative() {
        // 準備しちゃいな: ちゃい + the いなさい-contraction な stay in the
        // suru-verb span via the ちゃいな supplemental rule.
        let h = Harness::new();
        let text =
            "片付けと食器洗いは私がしておいてあげるから、はるちゃんは早く準備しちゃいなよ。";
        let span = h.lookup(text, cpos(text, "準備しちゃ"));
        assert_eq!(span.surface, "準備しちゃいな");
        assert_eq!(top_reading(&span), "じゅんび");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("contracted + casual imperative + ended up")
        );
    }

    #[test]
    fn adjective_njanai_takes_explanatory_negative() {
        // 良いんじゃないか: adjective stems take the copula tails too —
        // must not fall back to bare 良い or the coincidental 余韻.
        let h = Harness::new();
        let text = "逆に目立てて良いんじゃないかなぁ。";
        let span = h.lookup(text, cpos(text, "良いん"));
        assert_eq!(span.surface, "良いんじゃないか");
        assert_eq!(top_reading(&span), "よい");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("explanatory + negative + question")
        );
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
    fn shredded_oozappa_with_na_resolves() {
        // おおざっぱ tokenized as お|お|ざっぱな (unknown + adnominal な):
        // must reach 大雑把, not the おおい/多い misread of the bare おお.
        let h = Harness::new();
        let text = "その見た目通りにおおざっぱな性格なのか。";
        let span = h.lookup(text, cpos(text, "おおざっ"));
        assert_eq!(span.surface, "おおざっぱ");
        assert_eq!(top_reading(&span), "おおざっぱ");
        assert_eq!(span.entries[0].spellings[0], "大雑把");
    }

    #[test]
    fn ki_hover_covers_split_causative_expression() {
        // 気を悪くさせて tokenized 気|を|悪く|さ|せ|て: the せ continues the
        // さ (する-stem), and the whole span resolves to 気を悪くする —
        // not bare 悪い, which the longest-prefix tiebreak outranks.
        let h = Harness::new();
        let text = "わかんないけど。気を悪くさせてごめんね、本当に。";
        let span = h.lookup(text, cpos(text, "気を悪く"));
        assert_eq!(span.surface, "気を悪くさせて");
        assert_eq!(top_reading(&span), "きをわるくする");
        assert_eq!(span.entries[0].spellings[0], "気を悪くする");
        assert_eq!(span.deconjugated_from.as_deref(), Some("causative"));
    }

    #[test]
    fn suru_noun_tekureru_labels_benefactive() {
        // 手助けをしてくれる must read "suru + do for someone", not bare "suru".
        let h = Harness::new();
        let text = "豹馬(聞いてるぞ。5人で手助けをしてくれるんだよな)";
        let span = h.lookup(text, cpos(text, "手助け"));
        assert_eq!(span.surface, "手助けをしてくれる");
        assert_eq!(top_reading(&span), "てだすけ");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("suru + do for someone")
        );
    }
    #[test]
    fn souieba_expression_merges_across_split() {
        // そう|いえ|ば must reach the そういえば expression, not 相違.
        let h = Harness::new();
        let text = "そういえば、穹とは別のクラスになってしまった。";
        let span = h.lookup(text, cpos(text, "そうい"));
        assert_eq!(span.surface, "そういえば");
        assert_eq!(top_reading(&span), "そういえば");
    }

    #[test]
    fn kono_splits_from_koto() {
        // この|こと must stay この + こと, not このこ/海鼠子 — and この
        // must prefer 此の over the same-reading 九 via rentaishi agreement.
        let h = Harness::new();
        let text = "穹がこのことを知ったら、ますます学校に出てこないんじゃないかと、ひどく心配になる。";
        let span = h.lookup(text, cpos(text, "このこ"));
        assert_eq!(span.surface, "この");
        assert_eq!(span.entries[0].spellings[0], "此の");
    }

    #[test]
    fn chakuzushita_compound_verb_stays_one_span() {
        // 着|くず|し|た must reach 着崩す via kanji-sharing deconjugation.
        let h = Harness::new();
        let text = "少し着くずした制服に、あごヒゲがなんか少し学生離れしてる印象があるけど。";
        let span = h.lookup(text, cpos(text, "着くず"));
        assert_eq!(span.surface, "着くずした");
        assert_eq!(span.entries[0].spellings[0], "着崩す");
        assert_eq!(top_reading(&span), "きくずす");
    }

    #[test]
    fn fukai_demo_prefers_literal_stem() {
        // 不快でも must be 不快, not 深い + even-if.
        let h = Harness::new();
        let text = "でも、呼ばれ方に何かしらこだわりがあるわけじゃないし、彼女の愛嬌さもあって、特に不快でもなかった。";
        let span = h.lookup(text, cpos(text, "不快でも"));
        assert_eq!(span.surface, "不快");
        assert_eq!(top_reading(&span), "ふかい");
    }

    #[test]
    fn kangae_tenasasou_resolves_to_kangaeru() {
        // 考えてなさそう (tokens 考え|て|な|さ|そう) must reach 考える with
        // negative + hearsay labeling.
        let h = Harness::new();
        let text = "その無邪気そうな笑顔を見ていると実は何も考えてなさそうな雰囲気もなきにしもあらず。";
        let span = h.lookup(text, cpos(text, "考えてな"));
        assert_eq!(span.surface, "考えてなさそうな");
        assert_eq!(top_reading(&span), "かんがえる");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("negative + hearsay")
        );
    }

    #[test]
    fn katakana_hen_na_resolves_to_hen() {
        // ヘンな must be ヘン(変) + rentaikei な, not the ヘンナ plant.
        let h = Harness::new();
        let text = "ヘンな噂になると困るじゃないかぁ。";
        let span = h.lookup(text, cpos(text, "ヘンな"));
        assert_eq!(span.surface, "ヘン");
        assert_eq!(span.entries[0].spellings[0], "変");
    }

    #[test]
    fn nishinahyoron_merges_to_ni_naru_tono() {
        // に|なる|と must reach the になると expression.
        let h = Harness::new();
        let text = "ヘンな噂になると困るじゃないかぁ。";
        let span = h.lookup(text, cpos(text, "になると"));
        assert_eq!(span.surface, "になると");
        assert_eq!(top_reading(&span), "になると");
    }

    #[test]
    fn san_splits_from_ii() {
        // さん|いい|の must stay split, not さんい/三位 + いの.
        let h = Harness::new();
        let text = "渚さんいいの？迷惑じゃない？";
        let span = h.lookup(text, cpos(text, "さんいい"));
        assert_eq!(span.surface, "さん");
        let span = h.lookup(text, cpos(text, "さんいいの") + 2);
        assert_eq!(span.surface, "いい");
    }

    #[test]
    fn nan_darou_resolves_with_conjecture() {
        // 何|だろ|う must span 何だろう -> 何だ + conjecture, not bare 何だ.
        let h = Harness::new();
        let text = "何だろう、さっきもそうだったけど。";
        let span = h.lookup(text, cpos(text, "何だろう"));
        assert_eq!(span.surface, "何だろう");
        assert_eq!(span.entries[0].spellings[0], "何");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("conjecture")
        );
    }

    #[test]
    fn maji_ka_splits_to_maji() {
        // マジ|かっ must be マジ, not マジか/間近.
        let h = Harness::new();
        let text = "マジかっ！ちきしょー！俺より先に、何いい関係築いてんだよ！";
        let span = h.lookup(text, cpos(text, "マジか"));
        assert_eq!(span.surface, "マジ");
        assert_eq!(top_reading(&span), "まじ");
    }

    #[test]
    fn chikishoo_with_chouonpu_resolves_to_chikushou() {
        // ちきしょー (official 畜生 reading ちきしょう) must reach 畜生.
        let h = Harness::new();
        let text = "マジかっ！ちきしょー！俺より先に、何いい関係築いてんだよ！";
        let span = h.lookup(text, cpos(text, "ちきしょ"));
        assert_eq!(span.surface, "ちきしょー");
        assert_eq!(span.entries[0].spellings[0], "畜生");
    }

    #[test]
    fn sou_nanda_strips_to_sou() {
        // そう|な|ん|だ must span whole and resolve to そう + explanatory,
        // not cut at なん (遭難).
        let h = Harness::new();
        let text = "そ、そうなんだ。";
        let span = h.lookup(text, cpos(text, "そうなん"));
        assert_eq!(span.surface, "そうなんだ");
        assert_eq!(top_reading(&span), "そう");
        assert_eq!(span.deconjugated_from.as_deref(), Some("explanatory"));
    }

    #[test]
    fn youna_prefers_samana_over_you() {
        // 感じたような: 様な (rentaishi) must outrank 酔う, and the span
        // must not shorten to よう (様 lacks keiyodoshi, so rentaikei
        // shortening stays off).
        let h = Harness::new();
        let text = "感じたような。";
        let span = h.lookup(text, cpos(text, "ような"));
        assert_eq!(span.surface, "ような");
        assert_eq!(span.entries[0].spellings[0], "様な");
    }

    #[test]
    fn yokatta_with_emphatic_tsu_resolves_to_yoi() {
        // よかっ|たっ (past た + emphatic っ misread as 立つ) must reach
        // よかった -> 良い, not よか/余暇.
        let h = Harness::new();
        let text = "よかったっ、ハル君。";
        let span = h.lookup(text, cpos(text, "よかった"));
        assert_eq!(span.surface, "よかった");
        assert_eq!(top_reading(&span), "よい");
    }

    #[test]
    fn gomen_nasai_merges_across_split() {
        // ゴメン|な|さ|いね must reach ゴメンなさい -> 御免なさい.
        let h = Harness::new();
        let text = "みんな、ゴメンなさいね。";
        let span = h.lookup(text, cpos(text, "ゴメンな"));
        assert_eq!(span.surface, "ゴメンなさい");
        assert_eq!(span.entries[0].spellings[0], "御免なさい");
    }

    #[test]
    fn sou_hoihoi_splits_adverb_and_mimetic() {
        // そう + ほいほい must stay split (adverbs take no continuations),
        // not merge to そうほ/相補; ほいほい resolves whole.
        let h = Harness::new();
        let text = "はは、そうほいほい面白い。";
        let span = h.lookup(text, cpos(text, "そうほい"));
        assert_eq!(span.surface, "そう");
        let span = h.lookup(text, cpos(text, "ほいほい"));
        assert_eq!(span.surface, "ほいほい");
        assert_eq!(top_reading(&span), "ほいほい");
    }

    #[test]
    fn kaerimashou_volitional_reaches_kaeru() {
        // 帰り|ましょ|うかっ (う glued into mis-split adjective) must span
        // 帰りましょう -> 帰る, not cut before う.
        let h = Harness::new();
        let text = "お兄さんさあ、帰りましょうかっ!";
        let span = h.lookup(text, cpos(text, "帰りましょ"));
        assert_eq!(span.surface, "帰りましょう");
        assert_eq!(top_reading(&span), "かえる");
    }

    #[test]
    fn kiri_single_kanji_drops_coincidental_verbs() {
        // 霧がかかった: 霧 stays, but 切る/斬る/剪る (matched only through
        // the kana reading きり) must not clutter entries.
        let h = Harness::new();
        let text = "頭の中が霧がかかったようになっていて。";
        let span = h.lookup(text, cpos(text, "霧がかか"));
        assert_eq!(span.surface, "霧");
        assert_eq!(span.entries[0].spellings[0], "霧");
        assert!(!span.entries.iter().any(|e| e.spellings.first()
            .map_or(false, |s| s == "切る" || s == "着る" || s == "来る")),
            "coincidental kiru-verbs should be filtered, got {:?}",
            span.entries.iter().map(|e| e.spellings.first()).collect::<Vec<_>>());
    }

    #[test]
    fn sou_saseru_splits_to_sou() {
        // そうさせる must not resolve as one span to archaic 奏する;
        // hovering そう gives そう (させる -> する reachable separately).
        let h = Harness::new();
        let text = "そうさせる。";
        let span = h.lookup(text, cpos(text, "そうさせ"));
        assert_eq!(span.surface, "そう");
    }

    #[test]
    fn wakannai_slurred_negative_resolves() {
        // わかんない (slurred わからない) must reach 分かる.
        let h = Harness::new();
        let text = "わかんないけど。";
        let span = h.lookup(text, cpos(text, "わかんな"));
        assert_eq!(span.surface, "わかんないけど");
        assert_eq!(top_reading(&span), "わかる");
    }

    #[test]
    fn bonus_locks_koto_ni_suru_furisou_ayamannakute() {
        // Loose ends that already work: ことにする, 降りそう, あやまんなくて.
        let h = Harness::new();
        let span = h.lookup("ことにする。", cpos("ことにする。", "ことにする"));
        assert_eq!(span.surface, "ことにする");
        assert_eq!(span.entries[0].spellings[0], "事にする");
        let span = h.lookup("雨が降りそう。", cpos("雨が降りそう。", "降りそう"));
        assert_eq!(span.surface, "降りそう");
        assert_eq!(top_reading(&span), "おりる");
        let span = h.lookup("あやまんなくて良いよ。", cpos("あやまんなくて良いよ。", "あやまんな"));
        assert_eq!(top_reading(&span), "あやまる");
    }

    #[test]
    fn sou_nanda_etc_strip_universally() {
        // なんだ-family tails attach to any stem.
        let h = Harness::new();
        let span = h.lookup("そ、そうなんだ。", cpos("そ、そうなんだ。", "そうなん"));
        assert_eq!(span.surface, "そうなんだ");
        assert_eq!(top_reading(&span), "そう");
        // Verb + explanatory ん keeps the deliberate split (したんだ ->
        // した + んだ, never merged): verbs never absorb ん.
        let span = h.lookup("行くんですか。", cpos("行くんですか。", "行くんです"));
        assert_eq!(span.surface, "行く");
        // Noun stems do resolve whole through the tail.
        let span = h.lookup("本なんですか。", cpos("本なんですか。", "本なんで"));
        assert_eq!(span.surface, "本なんですか");
        assert_eq!(top_reading(&span), "ほん");
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
    fn obscure_literal_prefers_common_stem() {
        // Orphan literals swallowing common stems split back.
        let h = Harness::new();
        let span = h.lookup("そこにいる。", cpos("そこにいる。", "そこに"));
        assert_eq!(span.surface, "そこ");
        let span = h.lookup("渚さんと行く。", cpos("渚さんと行く。", "さんと"));
        assert_eq!(span.surface, "さん");
    }

    #[test]
    fn bound_volitional_u_reaches_verb() {
        let h = Harness::new();
        let text = "お兄さんさあ、帰りましょうかっ!";
        let span = h.lookup(text, cpos(text, "帰りましょ"));
        assert_eq!(span.surface, "帰りましょう");
        assert_eq!(top_reading(&span), "かえる");
    }

    #[test]
    fn tatsu_past_emphasis_splits_ta() {
        // たっ (past た + emphatic っ) must end at た.
        let h = Harness::new();
        let span = h.lookup("よかったっ。", cpos("よかったっ。", "よかった"));
        assert_eq!(span.surface, "よかった");
        assert_eq!(top_reading(&span), "よい");
    }

    #[test]
    fn nasai_after_noun_resolves() {
        // ゴメン|な|さ|いね must reach ゴメンなさい -> 御免なさい.
        let h = Harness::new();
        let text = "みんな、ゴメンなさいね。";
        let span = h.lookup(text, cpos(text, "ゴメンな"));
        assert_eq!(span.surface, "ゴメンなさい");
        assert_eq!(span.entries[0].spellings[0], "御免なさい");
    }

    #[test]
    fn causative_prefers_complete_stem_word() {
        // そうさせる must not resolve as one span to archaic 奏する.
        let h = Harness::new();
        let span = h.lookup("そうさせる。", cpos("そうさせる。", "そうさせ"));
        assert_eq!(span.surface, "そう");
    }

    #[test]
    fn sou_ieba_kanji_reaches_expression() {
        // そう|言え|ば (kanji tail) must merge via readings to そう言えば.
        let h = Harness::new();
        let text = "あー、そう言えば初めて。";
        let span = h.lookup(text, cpos(text, "そう言え"));
        assert_eq!(span.surface, "そう言えば");
        assert_eq!(span.entries[0].spellings[0], "そう言えば");
    }

    #[test]
    fn mono_ha_splits_to_mono() {
        // ものは must not resolve as one span to もの派.
        let h = Harness::new();
        let text = "足りないものはスーパーに。";
        let span = h.lookup(text, cpos(text, "ものは"));
        assert_eq!(span.surface, "もの");
    }

    #[test]
    fn naishi_single_token_prefers_nai() {
        // Conjunction-tagged ないし in dialogue position is ない + し.
        let h = Harness::new();
        let text = "体を鍛えてないし。";
        let span = h.lookup(text, cpos(text, "ないし"));
        assert_eq!(span.surface, "ない");
        assert_eq!(span.entries[0].spellings[0], "無い");
    }

    #[test]
    fn katakana_shai_outranks_homophones() {
        // Exact-script シャイ beats normalized-only 謝意/社医.
        let h = Harness::new();
        let text = "シャイなのか。";
        let span = h.lookup(text, cpos(text, "シャイ"));
        assert_eq!(span.surface, "シャイなのか");
        assert!(span.entries[0].readings.iter().any(|r| r == "シャイ"),
            "シャイ loanword should top, got {:?}", span.entries[0].readings.first());
    }

    #[test]
    fn suguni_prefix_shredding_resolves() {
        // す|ぐにどっか… (prefix + giant unknown) must reach すぐに/直ぐに.
        let h = Harness::new();
        let text = "すぐにどっか。";
        let span = h.lookup(text, cpos(text, "すぐに"));
        assert_eq!(span.surface, "すぐに");
        assert_eq!(span.entries[0].spellings[0], "直ぐに");
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

