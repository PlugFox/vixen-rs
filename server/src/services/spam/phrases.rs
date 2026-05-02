//! Curated spam phrase set ported from the Dart prototype
//! (`vixen/lib/src/anti_spam.dart` `$spamPhrases`).
//!
//! Each phrase is matched verbatim as a substring of the normalized message
//! body (see `super::normalize`). The Dart prototype strips stopwords before
//! matching, but several phrases contain stopwords (`в`, `на`, `details in dm`),
//! so the original code never matched them; we drop the stopword pass and
//! rely on the curated phrase list as-is.
//!
//! Default weight per matched phrase is 1.0; per-chat overrides come from
//! `chat_config.spam_weights` JSONB.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const RAW_PHRASES: &[&str] = &[
    // English spam phrases
    "make money",
    "work from home",
    "fast cash",
    "lose weight",
    "increase sales",
    "earn money",
    "details in dm",
    "details in pm",
    "details in private messages",
    "details in personal messages",
    "earn remotely",
    "earn online",
    "earn in network",
    "partners wanted",
    "buy now",
    "click here",
    "limited time",
    "act now",
    "best price",
    "free offer",
    "guarantee",
    "no obligation",
    // Срочность и ограниченность
    "только сегодня",
    "количество ограничено",
    "осталось немного",
    "последняя возможность",
    "успей купить",
    "акция заканчивается",
    "не упустите шанс",
    // Финансы и заработок
    "zarabotok",
    "быстрый заработок",
    "пассивный доход",
    "заработок в интернете",
    "работа на дому",
    "дополнительный доход",
    "высокий доход",
    "без вложений",
    "деньги без усилий",
    "заработок от",
    "доход от",
    "финансовая независимость",
    "миллион за месяц",
    "бизнес под ключ",
    // Скидки и цены
    "супер цена",
    "лучшая цена",
    "выгодное предложение",
    "без переплат",
    "специальное предложение",
    "уникальное предложение",
    // Здоровье и красота
    "похудение",
    "омоложение",
    "чудо-средство",
    "супер-эффект",
    "мгновенный результат",
    "похудеть без диет",
    "стопроцентный результат",
    "гарантированный эффект",
    "чудодейственный",
    // Призывы к действию
    "купить сейчас",
    "закажи сейчас",
    "звоните прямо сейчас",
    "перейдите по ссылке",
    // Гарантии и обещания
    "гарантия результата",
    "гарантированный доход",
    "стопроцентная гарантия",
    "без риска",
    // Инвестиции и криптовалюта
    "инвестиции под",
    "высокий процент",
    "криптовалюта",
    "биткоин",
    "майнинг",
    "прибыльные инвестиции",
    "доход от вложений",
    // Азартные игры
    "ставки на спорт",
    "беспроигрышная стратегия",
    // Кредиты и займы
    "кредит без справок",
    "займ без проверок",
    "деньги сразу",
    "одобрение без отказа",
    "кредит онлайн",
    "быстрые деньги",
    // Недвижимость
    "квартира в рассрочку",
    "без первоначального взноса",
    "материнский капитал",
    "ипотека без справок",
    // Образование и курсы
    "курсы похудения",
    "обучение заработку",
    "секреты успеха",
    "марафон похудения",
    "бесплатный вебинар",
    // Сетевой маркетинг
    "сетевой маркетинг",
    "бизнес возможность",
    "присоединяйся к команде",
    "построй свой бизнес",
    // Подозрительные обращения
    "дорогой клиент",
    "уважаемый пользователь",
    // Остальное
    "казино",
    "порно",
    "пассивного дохода",
    "ограниченное предложение",
    "действуйте сейчас",
    "бесплатное предложение",
    "без обязательств",
    "увеличение продаж",
    "писать в лc",
    "пишите в лс",
    "в лuчные сообщенuя",
    "личных сообщениях",
    "заработок удалённо",
    "заработок в сети",
    "для yдaлённoгo зaрaбoткa",
    "детали в лс",
    "ищу партнеров",
    "подробности в лс",
    "подробности в личке",
];

/// Default weight applied to every matched phrase when no override exists.
pub const DEFAULT_PHRASE_WEIGHT: f32 = 1.0;

/// Per-chat weight overrides, parsed from `chat_config.spam_weights` JSONB.
///
/// Schema: `{"<phrase>": <weight>, ...}`. Unknown phrases use
/// [`DEFAULT_PHRASE_WEIGHT`]. A weight of `0.0` effectively disables a phrase.
#[derive(Debug, Default, Clone)]
pub struct SpamWeights {
    overrides: HashMap<String, f32>,
}

impl SpamWeights {
    /// Build from a SQLx-decoded `serde_json::Value`. Anything that isn't a
    /// JSON object collapses to the empty override map (the spam pipeline then
    /// uses [`DEFAULT_PHRASE_WEIGHT`] for every match).
    pub fn from_json(value: &serde_json::Value) -> Self {
        let overrides = value
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_f64().map(|w| (k.clone(), w as f32)))
                    .collect()
            })
            .unwrap_or_default();
        Self { overrides }
    }

    pub fn weight_for(&self, phrase: &str) -> f32 {
        self.overrides
            .get(phrase)
            .copied()
            .unwrap_or(DEFAULT_PHRASE_WEIGHT)
    }
}

/// Curated spam phrase set with `score(normalized, &weights)` for the n-gram
/// step of the spam pipeline.
pub struct PhraseSet {
    phrases: HashSet<&'static str>,
}

impl PhraseSet {
    fn new() -> Self {
        Self {
            phrases: RAW_PHRASES.iter().copied().collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.phrases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.phrases.is_empty()
    }

    /// Returns the list of phrases that appear as substrings of `normalized`,
    /// sorted lexicographically. Sorting matters: the result lands in the
    /// `moderation_actions.reason` JSON, which is shown in audit/replay UIs
    /// and may be diff'd or hashed by downstream consumers — a `HashSet`
    /// iteration order would shuffle between processes for the same input.
    pub fn matches(&self, normalized: &str) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self
            .phrases
            .iter()
            .filter(|p| normalized.contains(*p))
            .copied()
            .collect();
        out.sort_unstable();
        out
    }

    /// Sum of per-phrase weights of every matched phrase, plus the matched
    /// list for explainability in the moderation ledger.
    pub fn score(&self, normalized: &str, weights: &SpamWeights) -> (f32, Vec<&'static str>) {
        let matched = self.matches(normalized);
        let score = matched.iter().map(|p| weights.weight_for(p)).sum();
        (score, matched)
    }
}

pub static PHRASES: LazyLock<PhraseSet> = LazyLock::new(PhraseSet::new);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn loads_at_least_100_phrases() {
        assert!(PHRASES.len() >= 100, "got {} phrases", PHRASES.len());
    }

    #[test]
    fn matches_english_phrase() {
        let matched = PHRASES.matches("hi everyone, click here for the best price");
        assert!(matched.contains(&"click here"));
        assert!(matched.contains(&"best price"));
    }

    #[test]
    fn matches_russian_phrase() {
        // Russian morphology means we only match nominative forms verbatim;
        // the curated set is shaped so the most common spam wording hits.
        let matched = PHRASES.matches("предлагаю быстрый заработок без вложений всем");
        assert!(matched.contains(&"быстрый заработок"));
        assert!(matched.contains(&"без вложений"));
    }

    #[test]
    fn ignores_clean_text() {
        let matched = PHRASES.matches("в пятницу созвон в 18:00, обсудим pr и тесты");
        assert!(matched.is_empty(), "false positives: {matched:?}");
    }

    #[test]
    fn default_weight_is_one() {
        let w = SpamWeights::default();
        assert_eq!(w.weight_for("anything"), DEFAULT_PHRASE_WEIGHT);
    }

    #[test]
    fn per_phrase_override_applies() {
        let w = SpamWeights::from_json(&json!({"buy now": 0.0, "click here": 2.5}));
        assert_eq!(w.weight_for("buy now"), 0.0);
        assert_eq!(w.weight_for("click here"), 2.5);
        assert_eq!(w.weight_for("partners wanted"), DEFAULT_PHRASE_WEIGHT);
    }

    #[test]
    fn score_sums_matched_weights() {
        let w = SpamWeights::from_json(&json!({"click here": 2.0}));
        let (score, matched) = PHRASES.score("click here for the best price", &w);
        // 2.0 (override) + 1.0 (default) = 3.0
        assert!(matched.contains(&"click here"));
        assert!(matched.contains(&"best price"));
        assert!((score - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn score_zero_on_clean_text() {
        let w = SpamWeights::default();
        let (score, matched) = PHRASES.score("hello world how are you", &w);
        assert!(matched.is_empty());
        assert_eq!(score, 0.0);
    }

    #[test]
    fn matches_returned_in_stable_order() {
        // The same input must produce the same matched-phrase order across
        // calls — `reason_json["ngram_phrases"]` is read by the audit UI and
        // may be diff'd. Repeat the query a few times to defeat a single
        // happy-path HashSet iteration.
        let body = "click here for the best price — buy now and act now today";
        let runs: Vec<Vec<&'static str>> = (0..8).map(|_| PHRASES.matches(body)).collect();
        for w in runs.windows(2) {
            assert_eq!(w[0], w[1], "matches() order is not stable");
        }
        let mut sorted = runs[0].clone();
        sorted.sort_unstable();
        assert_eq!(runs[0], sorted, "matches() should be sorted");
    }

    #[test]
    fn malformed_jsonb_collapses_to_empty() {
        let w = SpamWeights::from_json(&json!("not an object"));
        assert!(w.overrides.is_empty());
        assert_eq!(w.weight_for("buy now"), DEFAULT_PHRASE_WEIGHT);
    }
}
