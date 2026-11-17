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

/// What one answer cost at the base rates, in micro-dollars, and whether its model was known
/// (false: estimated at the dearest family).
pub fn cost(model: &str, u: &Usage) -> (u64, bool) {
    cost_with(model, u, &Extras::default())
}

/// What an answer reports beyond its token counts that changes its price.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    /// Server web searches (`usage.server_tool_use.web_search_requests`), $10 per thousand.
    pub web_searches: u64,
    /// Long prompts (past `LONG_CONTEXT` tokens) are priced at the long-context premium.
    pub long_context: bool,
    /// A faster or priority tier (`usage.service_tier: priority`, `usage.speed: fast`).
    pub fast: bool,
}
/// Prompts past this many tokens take the long-context premium (Anthropic: input x2, output
/// x1.5 past 200,000 tokens). Haiku 5.5 has its own two tiers instead.
pub const LONG_CONTEXT: u64 = 200_000;
/// Micro-dollars per server web search ($10 per 1,000 searches).
pub const WEB_SEARCH: u64 = 10_000;
/// The faster tier's price is not in the table: it is counted at six times the base price, the
/// dearest multiplier Anthropic lists for it, and marked estimated.
pub const FAST_FACTOR: u64 = 6;

/// What one answer cost, premiums included, and whether that is exact. A long prompt and the
/// faster tier are priced high and marked estimated, so a ceiling never reads less than was
/// spent.
pub fn cost_with(model: &str, u: &Usage, extras: &Extras) -> (u64, bool) {
    let prompt = u
        .input
        .saturating_add(u.cache_read)
        .saturating_add(u.cache_write)
        .saturating_add(u.cache_write_5m)
        .saturating_add(u.cache_write_1h);
    let (p, known) = price(model, prompt);
    let part = |tokens: u64, per_million: u64| {
        u64::try_from(u128::from(tokens) * u128::from(per_million) / 1_000_000).unwrap_or(u64::MAX)
    };
    let input = part(u.input, p.input)
        .saturating_add(part(u.cache_read, p.read))
        .saturating_add(part(u.cache_write_5m, p.write_5m))
        .saturating_add(part(
            u.cache_write_1h.saturating_add(u.cache_write),
            p.write_1h,
        ));
    let output = part(u.output, p.output);
    let tiered = model
        .to_ascii_lowercase()
        .replace('.', "-")
        .contains("haiku-5-5");
    let long = (extras.long_context || prompt > LONG_CONTEXT) && !tiered;
    let (input, output) = if long {
        (input.saturating_mul(2), output.saturating_mul(3) / 2)
    } else {
        (input, output)
    };
    let mut total = input
        .saturating_add(output)
        .saturating_add(extras.web_searches.saturating_mul(WEB_SEARCH));
    if extras.fast {
        total = total.saturating_mul(FAST_FACTOR);
    }
    (total, known && !long && !extras.fast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_model_is_priced_from_the_table_and_an_unknown_one_at_the_dearest() {
        // Opus 5.5: 100k input $0.40, 100k output $2.
        let u = Usage {
            input: 100_000,
            output: 100_000,
            ..Usage::default()
        };
        assert_eq!(cost("claude-opus-5-5", &u), (2_400_000, true));
        assert_eq!(cost("claude-opus-5-5-20261101", &u), (2_400_000, true));
        // Opus 5 is not Opus 5.5.
        assert_eq!(cost("claude-opus-5", &u), (3_000_000, true));
        assert_eq!(cost("claude-unknown-9", &u), (9_000_000, false));
    }

    #[test]
    fn cache_reads_and_writes_take_their_own_rates() {
        let u = Usage {
            cache_read: 50_000,
            cache_write_5m: 50_000,
            cache_write: 50_000,
            ..Usage::default()
        };
        // Sonnet 5.5 per million: read $0.10, 5m write $2.50, an unsplit write at the 1h rate $4.
        assert_eq!(cost("claude-sonnet-5-5", &u), (330_000, true));
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

    #[test]
    fn fable_5_2_is_priced_from_the_table() {
        let u = Usage {
            input: 1_000,
            output: 1_000,
            ..Usage::default()
        };
        // $8 and $40 per million.
        assert_eq!(cost("claude-fable-5-2", &u), (48_000, true));
        assert_eq!(cost("claude-fable-5.2-20261101", &u), (48_000, true));
    }

    #[test]
    fn premiums_are_priced_high_and_marked_estimated() {
        let u = Usage {
            input: 100_000,
            output: 100_000,
            ..Usage::default()
        };
        // Sonnet 5.5: $0.20 in, $1.00 out.
        assert_eq!(cost("claude-sonnet-5-5", &u), (1_200_000, true));
        let searches = Extras {
            web_searches: 3,
            ..Extras::default()
        };
        assert_eq!(
            cost_with("claude-sonnet-5-5", &u, &searches),
            (1_230_000, true)
        );
        let long = Usage {
            input: 300_000,
            output: 100_000,
            ..Usage::default()
        };
        // Input x2 ($1.20), output x1.5 ($1.50), estimated.
        assert_eq!(
            cost_with("claude-sonnet-5-5", &long, &Extras::default()),
            (2_700_000, false)
        );
        let fast = Extras {
            fast: true,
            ..Extras::default()
        };
        assert_eq!(
            cost_with("claude-sonnet-5-5", &u, &fast),
            (7_200_000, false)
        );
    }

    #[test]
    fn counts_past_any_real_answer_saturate_instead_of_wrapping() {
        let u = Usage {
            input: u64::MAX,
            output: u64::MAX,
            cache_read: u64::MAX,
            cache_write: u64::MAX,
            cache_write_5m: u64::MAX,
            cache_write_1h: u64::MAX,
        };
        assert_eq!(cost("claude-opus-4", &u), (u64::MAX, false));
    }
}
