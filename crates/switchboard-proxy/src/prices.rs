//! SB-401: what a Claude request costs at Anthropic's list price, from the token counts the
//! provider reports in its own answer. The prices are Anthropic's model pricing table
//! (https://platform.claude.com/docs/en/about-claude/pricing, read 2026-11-17), in micro-dollars
//! per million tokens. A model the table does not know is priced at the dearest family and marked
//! estimated, so a ceiling never reads a key's spend as lower than it was. A cache write whose
//! answer does not say which cache it went to is priced at the one-hour rate, the dearer one.

/// Micro-dollars per million tokens: base input, 5-minute cache write, 1-hour cache write, cache
/// read, output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Price {
    pub input: u64,
    pub write_5m: u64,
    pub write_1h: u64,
    pub read: u64,
    pub output: u64,
}
const fn usd(input: f64, write_5m: f64, write_1h: f64, read: f64, output: f64) -> Price {
    Price {
        input: (input * 1e6) as u64,
        write_5m: (write_5m * 1e6) as u64,
        write_1h: (write_1h * 1e6) as u64,
        read: (read * 1e6) as u64,
        output: (output * 1e6) as u64,
    }
}
/// Model id fragments, most specific first, and their price.
const TABLE: &[(&str, Price)] = &[
    ("fable-5-2", usd(8.0, 10.0, 16.0, 0.20, 40.0)),
    ("mythos-5-2", usd(8.0, 10.0, 16.0, 0.20, 40.0)),
    ("fable-5-1", usd(10.0, 12.5, 20.0, 0.25, 50.0)),
    ("mythos-5-1", usd(10.0, 12.5, 20.0, 0.25, 50.0)),
    ("fable-5", usd(10.0, 12.5, 20.0, 1.0, 50.0)),
    ("mythos-5", usd(10.0, 12.5, 20.0, 1.0, 50.0)),
    ("opus-5-5", usd(4.0, 5.0, 8.0, 0.20, 20.0)),
    ("opus-5", usd(5.0, 6.25, 10.0, 0.50, 25.0)),
    ("opus-4-8", usd(5.0, 6.25, 10.0, 0.50, 25.0)),
    ("opus-4-7", usd(5.0, 6.25, 10.0, 0.50, 25.0)),
    ("opus-4-6", usd(5.0, 6.25, 10.0, 0.50, 25.0)),
    ("opus-4-5", usd(5.0, 6.25, 10.0, 0.50, 25.0)),
    ("opus-4-1", usd(15.0, 18.75, 30.0, 1.50, 75.0)),
    ("opus-4", usd(15.0, 18.75, 30.0, 1.50, 75.0)),
    ("sonnet-5-5", usd(2.0, 2.5, 4.0, 0.10, 10.0)),
    ("sonnet-5", usd(2.0, 2.5, 4.0, 0.20, 10.0)),
    ("sonnet-4-6", usd(3.0, 3.75, 6.0, 0.30, 15.0)),
    ("sonnet-4-5", usd(3.0, 3.75, 6.0, 0.30, 15.0)),
    ("sonnet-4", usd(3.0, 3.75, 6.0, 0.30, 15.0)),
    ("haiku-4-5", usd(1.0, 1.25, 2.0, 0.10, 5.0)),
];
/// Haiku 5.5 is priced by the prompt's size: up to 100,000 tokens, and over it.
const HAIKU_5_5: (Price, Price) = (
    usd(0.10, 0.125, 0.20, 0.01, 0.50),
    usd(0.50, 0.625, 1.0, 0.05, 2.50),
);
const HAIKU_5_5_BREAK: u64 = 100_000;
/// The dearest family, for a model the table does not know.
const DEAREST: Price = usd(15.0, 18.75, 30.0, 1.50, 75.0);

/// One answer's token counts as the provider reported them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// Cache writes the answer did not split by cache.
    pub cache_write: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

/// The price of `model` and whether it was known; `prompt` is the request's whole input (for
/// Haiku 5.5's two tiers).
pub fn price(model: &str, prompt: u64) -> (Price, bool) {
    let model = model.to_ascii_lowercase().replace('.', "-");
    if model.contains("haiku-5-5") {
        return (
            if prompt > HAIKU_5_5_BREAK {
                HAIKU_5_5.1
            } else {
                HAIKU_5_5.0
            },
            true,
        );
    }
    TABLE
        .iter()
        .find(|(fragment, _)| model.contains(fragment))
        .map_or((DEAREST, false), |(_, p)| (*p, true))
}

/// What one answer cost, in micro-dollars, and whether its model was known (false: estimated at
/// the dearest family).
pub fn cost(model: &str, u: &Usage) -> (u64, bool) {
    let prompt = u.input + u.cache_read + u.cache_write + u.cache_write_5m + u.cache_write_1h;
    let (p, known) = price(model, prompt);
    let part = |tokens: u64, per_million: u64| (tokens as u128 * per_million as u128 / 1_000_000) as u64;
    let total = part(u.input, p.input)
        + part(u.output, p.output)
        + part(u.cache_read, p.read)
        + part(u.cache_write_5m, p.write_5m)
        + part(u.cache_write_1h + u.cache_write, p.write_1h);
    (total, known)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_model_is_priced_from_the_table_and_an_unknown_one_at_the_dearest() {
        // Opus 5.5: 1M input $4, 1M output $20.
        let u = Usage {
            input: 1_000_000,
            output: 1_000_000,
            ..Usage::default()
        };
        assert_eq!(cost("claude-opus-5-5", &u), (24_000_000, true));
        assert_eq!(cost("claude-opus-5-5-20261101", &u), (24_000_000, true));
        // Opus 5 is not Opus 5.5.
        assert_eq!(cost("claude-opus-5", &u), (30_000_000, true));
        assert_eq!(cost("claude-unknown-9", &u), (90_000_000, false));
    }

    #[test]
    fn cache_reads_and_writes_take_their_own_rates() {
        let u = Usage {
            cache_read: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write: 1_000_000,
            ..Usage::default()
        };
        // Sonnet 5.5: read $0.10, 5m write $2.50, an unsplit write at the 1h rate $4.
        assert_eq!(cost("claude-sonnet-5-5", &u), (6_600_000, true));
    }

    #[test]
    fn haiku_5_5_is_dearer_past_a_hundred_thousand_prompt_tokens() {
        let small = Usage {
            input: 100_000,
            ..Usage::default()
        };
        let large = Usage {
            input: 100_001,
            ..Usage::default()
        };
        assert_eq!(cost("claude-haiku-5-5", &small).0, 10_000);
        assert_eq!(cost("claude-haiku-5-5", &large).0, 50_000);
    }
}
