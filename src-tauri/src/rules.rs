/// Data tables for span building and candidate lookup. Kept in one place so
/// new constructions (completions, tails, auxiliaries) are added as data,
/// not control flow.

// Character count (not morpheme count) a phrase match can span. This is
// purely a performance/sanity cap on how far the longest-match scan looks
// ahead from a given position — it is NOT a linguistic boundary. JL does
// not use POS tagging or any tokenizer to decide where a match is allowed
// to end; the dictionary (plus deconjugation) is the only thing that
// decides that. Whatever doesn't resolve to a real entry at a given
// length just falls through to a shorter candidate at the same position.
pub(crate) const MAX_CHARS_COMBINED: usize = 16;

/// Group verbs that directly inflect a te/で-form within a constructions
/// phrase — て+いる (teiru), て+くる (inceptive), て+しまう (completion),
/// て+みる (attempt), て+おく (preparative), and the benefactive て+くれる/
/// あげる/もらう. These stay inside the suru-noun phrase (調査している ->
/// 調査), while an independent verb after て (徹底して伏せた -> 徹底 + 伏せた)
/// starts a fresh clause and does not.
pub(crate) const TE_AUX_VERBS: &[&str] = &[
    "いる", "くる", "いく", "おく", "しまう", "みる", "くれる", "あげる", "もらう",
];

/// Contracted てしまう/でしまう auxiliaries (～ちゃう/じゃう and their
/// inflections, base ちゃう/じゃう when MeCab knows them). They continue a
/// suru-verb chain exactly like てる does (遅刻しちゃう -> 遅刻,
/// 準備しちゃいな -> 準備).
pub(crate) const CONTRACTION_AUX_VERBS: &[&str] = &["ちゃう", "じゃう", "ちまう", "じまう"];

/// Inflected surfaces of the ～ちゃう/じゃう contractions, for when MeCab
/// mis-analyzes the piece as an unknown token (base "*") instead.
pub(crate) const CONTRACTION_SURFACES: &[&str] = &[
    "ちゃう", "ちゃっ", "ちゃい", "ちゃえ", "ちゃお", "ちゃわ", "ちゃいな", "ちゃいなさい",
    "じゃう", "じゃっ", "じゃい", "じゃえ", "じゃお", "じゃわ", "じゃいな", "じゃいなさい",
    "ちまう", "ちまっ", "ちまい", "ちまえ", "ちまお", "ちまわ",
    "じまう", "じまっ", "じまい", "じまえ", "じまお", "じまわ",
];

/// Fixed-expression completions across token splits: なんで ("why")
/// split as な|んで, だった as だっ|た, でした as でし|た, になると
/// as に|なる|と, そういえば as そう|いえ|ば (or そう|言え|ば),
/// なさい as な|さ|いね. The heads are either locked to a single
/// token (function words) or cut off by a verb guard (そう + いえ)
/// — but the merged forms are real entries, so allow exactly the
/// merged span; longest-first lookup falls back to the pieces when
/// it doesn't apply. Exact-match only (never prefixes): 相手なんだ
/// splits as な|ん|だ, where な+ん ("なん") must NOT merge. Tails may
/// span several tokens (いえ|ば), may match normalized surfaces
/// (言え|ば), and may complete mid-token (な|さ|いね for ゴメンなさい
/// の さい). Function-head pairs also run at later token boundaries
/// (ゴメン|な|さ|いね from a ゴメン cursor), so fixed expressions
/// stay reachable wherever the user points inside them.
pub(crate) const COMPLETIONS: &[(&str, &str, bool)] = &[
    // (head, tail, function_head_only)
    ("な", "んで", true),
    ("だっ", "た", true),
    ("だっ", "たら", true),
    // Conjecture だろう (どうなんだろう?): だろ and う are split
    // tokens (だろ is 助動詞, う a separate auxiliary), and the
    // sentence-final-う rule needs a multi-char う token, so the
    // conjecture never formed — だろ stood alone and う resolved
    // to 兎. だろ is a function word, so the head is locked to it.
    ("だろ", "う", true),
    // なかった (negative + past) splits as なかっ|た, and the なかっ
    // token is a 助動詞 — function-locked, so the negative past never
    // forms on its own (らしくなかった -> なか + た).
    ("なかっ", "た", true),
    ("でし", "た", true),
    ("に", "なる", true),
    ("そう", "いえば", false),
    ("そう", "いや", false),
    ("な", "さい", true),
    // なめんな (don't lick/underestimate — 舐めるな slurred,
    // shredded な|めん|な): the めん shred never forms a span on
    // its own. Any な head may start it, so don't require a
    // function head — the exact めんな tail plus longest-first
    // fallback keep it safe.
    ("な", "めんな", false),
    // てめえ/てめー/てめっ ("you", vulgar): MeCab shreds them
    // て|め|え and て|め|ー (unknown noun) or て|め|っ (verb),
    // with て function-locked, so the span never forms. The
    // tail matches readings (めえ), the chouonpu surface (めー,
    // via the unknown-reading fallback), or the sokuon (めっ).
    ("て", "めえ", true),
    ("て", "めー", true),
    ("て", "めっ", true),
    // やがって (終わらせ|や|がって, auxiliary -yagaru): MeCab
    // splits や (particle) from がって, and the がって token
    // alone only reaches がる — the attachment never forms
    // without the head. The head is the particle, so lock it.
    ("や", "がって", true),
    // けど slurred as け|どぉ (or け|ど|ぉ): the け head is
    // function-locked (助詞), so the merged conjunction never
    // forms on its own. The ど tail covers the ど|ぉ split (via
    // the mid-token arm), どぉ the single-token form.
    ("け", "ど", true),
    ("け", "どぉ", true),
    // しかない ("only") splits as しか|ない, and the しか head
    // is function-locked — without the merge the expression
    // never forms (the ない tail matches readings; 鹿しかいない
    // stays safe because the tail must equal ない exactly).
    ("しか", "ない", true),
    // だけど ("but") splits as だけ|ど with the same lock.
    ("だけ", "ど", true),
    // ちゃっちゃと ("quickly") shreds as ちゃっ|ちゃ|と with a
    // function-locked ちゃっ head, so the adverb never forms
    // (and ちゃう wins its list). Any ちゃ head may start the
    // ちゃ-pairs: the exact tail still has to match, so お茶と
    // (と != ちゃと) never merges.
    ("ちゃっ", "ちゃと", false),
    ("ちゃっ", "ちゃっと", false),
    ("ちゃ", "ちゃと", false),
    ("ちゃ", "ちゃっと", false),
    // まずは ("first of all"): UniDic splits まず|は (IPAdic keeps one
    // adverb token), and the は never extends on its own. The tail
    // matches readings (は reads わ). Any head may start it — the exact
    // tail plus longest-first fallback keep it safe.
    ("まず", "わ", false),
    // いいよ ("it's fine"): UniDic splits いい|よ|っ (IPAdic keeps one
    // token, so the sub-token ends used to reach いいよ). The よ never
    // extends on its own. Any head may start it — longest-first falls
    // back when no expression entry resolves.
    ("いい", "よ", false),
    // もう一つ ("one more"): UniDic splits もう|一|つ (一 is a 数詞
    // noun, つ a suffix). The tail accumulates readings (ひと|つ).
    ("もう", "ひとつ", false),
    // Conditional として ("assuming"): UniDic splits と|し|て. と
    // followed by し|て is always this construction or と+する-te.
    ("と", "して", true),
    // でも ("even"): UniDic splits で|も (風船でも割れる must stay
    // 風船 / でも / 割れる). でも is a real entry; longest-first falls
    // back when it doesn't apply.
    ("で", "も", true),
    // だろうか ("I wonder"): UniDic keeps だろう whole but か never
    // attaches on its own. だろう|か is always this construction.
    ("だろう", "か", true),
    // だから/ですから ("so, because"): the causal から never attaches
    // on its own, and だ/です are function-locked. A だ never precedes
    // from-から (that から attaches to nouns), so every だ|から split
    // is this construction. (だろうから has no headword — it correctly
    // stays だろう|か|ら.)
    ("だ", "から", true),
    ("です", "から", true),
    // かもしれない ("maybe"): UniDic splits か|も|しれ|ない with
    // function-locked heads, so the expression never forms (あるの
    // absorbs のか instead). Headed at か, not の: の is the
    // explanatory particle (its own span) and isn't part of the
    // headword (かも知れない/かもしれない). The tail accumulates
    // readings across all four tokens; polite かもしれません works
    // the same way through the same tail string.
    ("か", "もしれない", true),
    // でしたり (copula continuative + たり, 彼からでしたり): UniDic
    // splits で|し|たり with a 助詞 で, so the copula-たり merge never
    // forms. で followed by し|たり is always this construction.
    ("で", "したり", true),
    // ゴメンなさい: UniDic keeps なさい whole as a 動詞 (base なさる),
    // so the な|さ|い completion never fires and the expression splits.
    // Any head may start it — the exact tail keeps it safe.
    ("ゴメン", "なさい", false),
    ("ごめん", "なさい", false),
];

/// Single-char particles a clipped emphasis can attach to (sokuon-span
/// shortening: ですっ -> です, いいよっ -> いいよ).
pub(crate) const SOKUON_TAIL_PARTICLES: &[char] =
    &['よ', 'ね', 'な', 'か', 'は', 'も', 'と', 'に', 'で', 'が', 'を', 'や'];

/// Continuative particles (ながら/たり/だり/がてら/つつ) never split
/// either — they inflect the verb rather than casing a noun.
pub(crate) const CONTINUATIVE: &[&str] = &["ながら", "たり", "だり", "がてら", "つつ"];

/// A whole-candidate しやがった-family is する + the vulgar auxiliary
/// やがる (MeCab shreds it し|や|がっ|た, so it never resolves
/// normally). Look it up as する as well. The same auxiliary on a
/// longer surface (バカにしやがった) is バカにする + やがった: strip the
/// しやがる-family suffix and restore the suru verb, so the dictionary
/// compound (馬鹿にする) resolves whole. Other verbs + やがる
/// (食べやがった) degrade to front-verb + junk — future work.
pub(crate) const SHIYAGARU_FORMS: &[&str] = &[
    "しやがる",
    "しやがった",
    "しやがって",
    "しやがり",
    "しやがれ",
    "しやがろう",
    "しやがらない",
    "しやがります",
    "しやがりました",
    "しやがりません",
];

/// Bare "suru" means no rule chain reached the verb (てくれる/
/// てもらう tails): name the grammaticalized tail auxiliaries
/// directly so 手助けをしてくれる reads "suru + do for someone"
/// instead of a bare "suru".
pub(crate) const TAIL_AUX_LABELS: &[(&str, &str)] = &[
    ("くれる", "do for someone"),
    ("あげる", "do for someone"),
    ("もらう", "get someone to do"),
    ("しまう", "ended up"),
    ("みる", "try"),
    ("おく", "in advance"),
];

/// Explanatory/copula tails attach to a verb stem in speech but have no
/// deconjugation rules of their own, so a whole span like
/// 寝てたんじゃなかった would otherwise fall back to the shorter 寝てた.
/// Strip the longest matching tails, deconjugate the stem normally, and
/// re-attach the tail descriptions to the label. Verb-start spans only:
/// noun+copula (学生だった, ジイさんじゃない) keeps its dedicated copula
/// path, and prohibitive んじゃない ("don't", one rule step) keeps winning
/// ties via the +2 step penalty here.
pub(crate) const COPULA_TAILS: &[(&str, &str)] = &[
    ("んじゃなかった", "explanatory + negative + past"),
    ("んじゃないか", "explanatory + negative + question"),
    ("じゃないか", "negative + question"),
    ("じゃなかった", "negative + past"),
    ("んじゃない", "explanatory + negative"),
    ("じゃない", "negative"),
    ("んです", "explanatory + polite"),
    ("のです", "explanatory + polite"),
    ("んだった", "explanatory + past"),
    ("のだ", "explanatory"),
    ("のか", "question"),
    ("んだ", "explanatory"),
];

/// Conjecture だろう/でしょう (何だろう -> 何だ + conjecture): attaches
/// to verbs, adjectives, and nouns alike, but JL's だろう rule only
/// rewrites a bare だろう, so longer spans never resolve it. Adverbs
/// keep their current behavior (そうだろう -> そう) so JL-covered forms
/// stay stable.
pub(crate) const CONJECTURE_TAILS: &[(&str, &str)] = &[
    ("だろう", "conjecture"),
    ("でしょう", "conjecture (polite)"),
];

/// なんだ-family tails: なんです/なのか/なのです/んですか/なのですか/っけ
/// attach to any stem, so unlike copula tails they need no gate — the
/// suffixes themselves are unambiguous. Plain なんだ is gated to
/// adverbial stems (そう/こんな/何 + なんだ): after nouns it stays split
/// (相手なんだ -> 相手 + な + ん + だ by deliberate design).
pub(crate) const NANDA_TAILS: &[(&str, &str)] = &[
    ("なんです", "explanatory + polite"),
    ("なのか", "question"),
    ("なのです", "explanatory + polite"),
    ("んですか", "question + polite"),
    ("なのですか", "question + polite"),
    ("っけ", "recall"),
];

pub(crate) const NANDA_ADVERBIAL_TAILS: &[(&str, &str)] = &[("なんだ", "explanatory")];
