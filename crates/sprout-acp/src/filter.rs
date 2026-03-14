//! Content filtering and subscription rule matching.
//!
//! Responsibilities:
//! - Building an evalexpr context from a Nostr event
//! - Evaluating boolean filter expressions with a hard timeout
//! - Matching events against ordered subscription rules (first match wins)

use std::time::Duration;

use tracing::warn;

// ── Errors ────────────────────────────────────────────────────────────────────

/// Errors that can occur during filter expression evaluation.
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    #[error("expression too long ({len} bytes, max {max})")]
    ExpressionTooLong { len: usize, max: usize },
    #[error("evaluation timed out")]
    Timeout,
    #[error("evaluation error: {0}")]
    EvalError(String),
}

// ── FilterContext ─────────────────────────────────────────────────────────────

/// Variables extracted from a Nostr event for use in filter expressions.
#[derive(Debug, Clone)]
pub struct FilterContext {
    /// Event content (message body).
    pub content: String,
    /// Event author pubkey as hex string.
    pub author: String,
    /// Nostr event kind number.
    pub kind: u32,
    /// Channel UUID as string.
    pub channel_id: String,
    /// Event `created_at` unix timestamp.
    pub timestamp: u64,
}

impl FilterContext {
    /// Build a `FilterContext` from a Nostr event and its channel UUID.
    pub fn from_event(event: &nostr::Event, channel_id: uuid::Uuid) -> Self {
        Self {
            content: event.content.clone(),
            author: event.pubkey.to_hex(),
            kind: event.kind.as_u16() as u32,
            channel_id: channel_id.to_string(),
            timestamp: event.created_at.as_u64(),
        }
    }
}

// ── SubscriptionRule ──────────────────────────────────────────────────────────

/// Scope of channels a subscription rule applies to.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ChannelScope {
    /// The literal string `"all"` — matches every channel.
    All(String),
    /// An explicit list of channel UUID strings.
    List(Vec<String>),
}

impl ChannelScope {
    /// Returns `true` if this scope covers the given channel UUID.
    ///
    /// `ChannelScope::All` only matches when the inner string is exactly `"all"`.
    pub fn matches(&self, channel_id: &uuid::Uuid) -> bool {
        match self {
            ChannelScope::All(s) => s == "all",
            ChannelScope::List(ids) => ids.iter().any(|id| id == &channel_id.to_string()),
        }
    }
}

/// A single subscription rule from the agent config.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SubscriptionRule {
    /// Human-readable rule name; used as fallback `prompt_tag`.
    pub name: String,
    /// Which channels this rule applies to.
    pub channels: ChannelScope,
    /// Nostr event kinds to match. Empty = wildcard (all kinds).
    #[serde(default)]
    pub kinds: Vec<u32>,
    /// If `true`, the event must contain a `p` tag referencing the agent pubkey.
    #[serde(default)]
    pub require_mention: bool,
    /// Optional evalexpr boolean expression for fine-grained filtering.
    #[serde(default)]
    pub filter: Option<String>,
    /// Tag passed to the prompt template. Falls back to `name` if absent.
    #[serde(default)]
    pub prompt_tag: Option<String>,
}

// ── MatchedRule ───────────────────────────────────────────────────────────────

/// The result of a successful rule match.
#[derive(Debug, Clone)]
pub struct MatchedRule {
    /// Zero-based index of the matching rule in the rules slice.
    #[cfg_attr(not(test), allow(dead_code))]
    pub rule_index: usize,
    /// Prompt tag to use (rule's `prompt_tag` or its `name`).
    pub prompt_tag: String,
}

// ── evaluate_filter ───────────────────────────────────────────────────────────

/// Maximum expression length accepted by `evaluate_filter`.
///
/// Bounds worst-case O(2^n) evaluation paths. The spawn_blocking thread cannot
/// be cancelled after a timeout fires, so we cap length before dispatching.
const MAX_EXPR_LEN: usize = 4096;

/// Maximum wall-clock time allowed for a single evalexpr evaluation.
const EVAL_TIMEOUT: Duration = Duration::from_millis(100);

/// Evaluate a boolean filter expression against a `FilterContext`.
///
/// - Caps expression length at [`MAX_EXPR_LEN`] bytes.
/// - Runs evaluation on a blocking thread with a [`EVAL_TIMEOUT`] hard timeout.
/// - Registers custom string helpers: `str_contains`, `str_starts_with`,
///   `str_ends_with`, `str_len` (duplicated intentionally from sprout-workflow).
pub async fn evaluate_filter(expr: &str, ctx: &FilterContext) -> Result<bool, FilterError> {
    if expr.len() > MAX_EXPR_LEN {
        return Err(FilterError::ExpressionTooLong {
            len: expr.len(),
            max: MAX_EXPR_LEN,
        });
    }

    let eval_ctx = build_eval_context(ctx).map_err(FilterError::EvalError)?;
    let expr_owned = expr.to_owned();

    let result = tokio::time::timeout(
        EVAL_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            evalexpr::eval_boolean_with_context(&expr_owned, &eval_ctx)
        }),
    )
    .await
    .map_err(|_| FilterError::Timeout)?
    .map_err(|e| FilterError::EvalError(format!("eval task panicked: {e}")))?
    .map_err(|e| FilterError::EvalError(e.to_string()))?;

    Ok(result)
}

/// Build an `evalexpr::HashMapContext` from a `FilterContext`.
///
/// Variables exposed to expressions:
///
/// | Name         | Type   | Source                    |
/// |--------------|--------|---------------------------|
/// | `content`    | string | `event.content`           |
/// | `author`     | string | `event.pubkey` (hex)      |
/// | `kind`       | int    | `event.kind`              |
/// | `channel_id` | string | channel UUID              |
/// | `timestamp`  | int    | `event.created_at`        |
///
/// Also registers `str_contains`, `str_starts_with`, `str_ends_with`,
/// `str_len` — duplicated from sprout-workflow intentionally so this crate
/// has no runtime dependency on sprout-workflow.
fn build_eval_context(ctx: &FilterContext) -> Result<evalexpr::HashMapContext, String> {
    use evalexpr::*;

    let mut eval_ctx = HashMapContext::new();

    // ── Custom string functions ───────────────────────────────────────────────
    // evalexpr v11 does not ship these helpers; register them manually.

    eval_ctx
        .set_function(
            "str_contains".into(),
            Function::new(|args| {
                let args = args.as_fixed_len_tuple(2)?;
                let haystack = args[0].as_string()?;
                let needle = args[1].as_string()?;
                Ok(Value::Boolean(haystack.contains(needle.as_str())))
            }),
        )
        .map_err(|e| e.to_string())?;

    eval_ctx
        .set_function(
            "str_starts_with".into(),
            Function::new(|args| {
                let args = args.as_fixed_len_tuple(2)?;
                let s = args[0].as_string()?;
                let prefix = args[1].as_string()?;
                Ok(Value::Boolean(s.starts_with(prefix.as_str())))
            }),
        )
        .map_err(|e| e.to_string())?;

    eval_ctx
        .set_function(
            "str_ends_with".into(),
            Function::new(|args| {
                let args = args.as_fixed_len_tuple(2)?;
                let s = args[0].as_string()?;
                let suffix = args[1].as_string()?;
                Ok(Value::Boolean(s.ends_with(suffix.as_str())))
            }),
        )
        .map_err(|e| e.to_string())?;

    eval_ctx
        .set_function(
            "str_len".into(),
            Function::new(|arg| {
                let s = arg.as_string()?;
                Ok(Value::Int(s.len() as i64))
            }),
        )
        .map_err(|e| e.to_string())?;

    // ── Event variables ───────────────────────────────────────────────────────

    eval_ctx
        .set_value("content".into(), Value::String(ctx.content.clone()))
        .map_err(|e| e.to_string())?;
    eval_ctx
        .set_value("author".into(), Value::String(ctx.author.clone()))
        .map_err(|e| e.to_string())?;
    eval_ctx
        .set_value("kind".into(), Value::Int(ctx.kind as i64))
        .map_err(|e| e.to_string())?;
    eval_ctx
        .set_value("channel_id".into(), Value::String(ctx.channel_id.clone()))
        .map_err(|e| e.to_string())?;
    eval_ctx
        .set_value("timestamp".into(), Value::Int(ctx.timestamp as i64))
        .map_err(|e| e.to_string())?;

    Ok(eval_ctx)
}

// ── match_event ───────────────────────────────────────────────────────────────

/// Match a Nostr event against an ordered list of subscription rules.
///
/// Rules are evaluated in order; the first rule whose conditions all pass
/// wins. Returns `None` if no rule matches.
///
/// # Matching logic (per rule)
///
/// 1. **channels** — if not `"all"`, the event's channel UUID must be in the list.
/// 2. **kinds** — if non-empty, the event kind must be in the list.
/// 3. **require_mention** — if `true`, a `p` tag matching `agent_pubkey_hex` must exist.
/// 4. **filter** — if `Some`, the evalexpr expression must evaluate to `true`.
///    Evaluation errors are logged as warnings and treated as non-matching.
pub async fn match_event(
    event: &nostr::Event,
    channel_id: uuid::Uuid,
    rules: &[SubscriptionRule],
    agent_pubkey_hex: &str,
) -> Option<MatchedRule> {
    let filter_ctx = FilterContext::from_event(event, channel_id);

    for (index, rule) in rules.iter().enumerate() {
        // 1. Channel scope check.
        if !rule.channels.matches(&channel_id) {
            continue;
        }

        // 2. Kind filter (empty = wildcard).
        if !rule.kinds.is_empty() && !rule.kinds.contains(&(event.kind.as_u16() as u32)) {
            continue;
        }

        // 3. Mention check — look for a `p` tag whose value equals agent_pubkey_hex.
        if rule.require_mention {
            let mentioned = event.tags.iter().any(|tag| {
                tag.kind().to_string() == "p" && (tag.content() == Some(agent_pubkey_hex))
            });
            if !mentioned {
                continue;
            }
        }

        // 4. Optional evalexpr filter expression.
        if let Some(expr) = &rule.filter {
            match evaluate_filter(expr, &filter_ctx).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    warn!(rule = %rule.name, error = %e, "filter expression error; skipping rule");
                    continue;
                }
            }
        }

        // All checks passed — this rule wins.
        let prompt_tag = rule.prompt_tag.clone().unwrap_or_else(|| rule.name.clone());

        return Some(MatchedRule {
            rule_index: index,
            prompt_tag,
        });
    }

    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};
    use uuid::Uuid;

    /// Build a minimal test event with the given kind and content.
    fn make_event(kind: u32, content: &str) -> nostr::Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::Custom(kind as u16), content, [])
            .sign_with_keys(&keys)
            .unwrap()
    }

    /// Build a test event with an explicit `p` tag.
    fn make_event_with_p_tag(kind: u32, content: &str, p_hex: &str) -> nostr::Event {
        let keys = Keys::generate();
        let p_tag = Tag::parse(&["p", p_hex]).expect("tag parse");
        EventBuilder::new(Kind::Custom(kind as u16), content, [p_tag])
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn any_channel() -> Uuid {
        Uuid::new_v4()
    }

    // ── FilterContext ─────────────────────────────────────────────────────────

    #[test]
    fn test_filter_context_from_event() {
        let event = make_event(40001, "hello world");
        let channel_id = any_channel();
        let ctx = FilterContext::from_event(&event, channel_id);

        assert_eq!(ctx.content, "hello world");
        assert_eq!(ctx.author, event.pubkey.to_hex());
        assert_eq!(ctx.kind, 40001);
        assert_eq!(ctx.channel_id, channel_id.to_string());
        assert_eq!(ctx.timestamp, event.created_at.as_u64());
    }

    // ── evaluate_filter ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_filter_str_contains() {
        let event = make_event(40001, "P1 incident in production");
        let ctx = FilterContext::from_event(&event, any_channel());

        let result = evaluate_filter(r#"str_contains(content, "P1")"#, &ctx)
            .await
            .unwrap();
        assert!(result);

        let result = evaluate_filter(r#"str_contains(content, "P2")"#, &ctx)
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_evaluate_filter_kind_check() {
        let event = make_event(40001, "some content");
        let ctx = FilterContext::from_event(&event, any_channel());

        let result = evaluate_filter("kind == 40001", &ctx).await.unwrap();
        assert!(result);

        let result = evaluate_filter("kind == 1", &ctx).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_evaluate_filter_too_long() {
        let event = make_event(40001, "content");
        let ctx = FilterContext::from_event(&event, any_channel());

        let long_expr = "a".repeat(MAX_EXPR_LEN + 1);
        let err = evaluate_filter(&long_expr, &ctx).await.unwrap_err();

        assert!(matches!(
            err,
            FilterError::ExpressionTooLong { len, max }
            if len == MAX_EXPR_LEN + 1 && max == MAX_EXPR_LEN
        ));
    }

    // ── match_event ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_match_event_first_match_wins() {
        let event = make_event(40001, "hello");
        let channel_id = any_channel();

        let rules = vec![
            SubscriptionRule {
                name: "first".into(),
                channels: ChannelScope::All("all".into()),
                kinds: vec![],
                require_mention: false,
                filter: None,
                prompt_tag: Some("tag-first".into()),
            },
            SubscriptionRule {
                name: "second".into(),
                channels: ChannelScope::All("all".into()),
                kinds: vec![],
                require_mention: false,
                filter: None,
                prompt_tag: Some("tag-second".into()),
            },
        ];

        let matched = match_event(&event, channel_id, &rules, "").await.unwrap();
        assert_eq!(matched.rule_index, 0);
        assert_eq!(matched.prompt_tag, "tag-first");
    }

    #[tokio::test]
    async fn test_match_event_kind_filter() {
        let event = make_event(40001, "hello");
        let channel_id = any_channel();

        let rules = vec![
            SubscriptionRule {
                name: "wrong-kind".into(),
                channels: ChannelScope::All("all".into()),
                kinds: vec![1],
                require_mention: false,
                filter: None,
                prompt_tag: None,
            },
            SubscriptionRule {
                name: "right-kind".into(),
                channels: ChannelScope::All("all".into()),
                kinds: vec![40001],
                require_mention: false,
                filter: None,
                prompt_tag: Some("matched".into()),
            },
        ];

        let matched = match_event(&event, channel_id, &rules, "").await.unwrap();
        assert_eq!(matched.rule_index, 1);
        assert_eq!(matched.prompt_tag, "matched");
    }

    #[tokio::test]
    async fn test_match_event_require_mention() {
        let agent_pubkey = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

        let event_no_mention = make_event(40001, "hello");
        let event_with_mention = make_event_with_p_tag(40001, "hello", agent_pubkey);
        let channel_id = any_channel();

        let rules = vec![SubscriptionRule {
            name: "mention-only".into(),
            channels: ChannelScope::All("all".into()),
            kinds: vec![],
            require_mention: true,
            filter: None,
            prompt_tag: Some("mentioned".into()),
        }];

        // Without mention — no match.
        let result = match_event(&event_no_mention, channel_id, &rules, agent_pubkey).await;
        assert!(result.is_none());

        // With mention — matches.
        let matched = match_event(&event_with_mention, channel_id, &rules, agent_pubkey)
            .await
            .unwrap();
        assert_eq!(matched.prompt_tag, "mentioned");
    }

    #[tokio::test]
    async fn test_match_event_no_match() {
        let event = make_event(1, "hello");
        let channel_id = any_channel();

        let rules = vec![SubscriptionRule {
            name: "kind-40001-only".into(),
            channels: ChannelScope::All("all".into()),
            kinds: vec![40001],
            require_mention: false,
            filter: None,
            prompt_tag: None,
        }];

        let result = match_event(&event, channel_id, &rules, "").await;
        assert!(result.is_none());
    }

    // ── ChannelScope ──────────────────────────────────────────────────────────

    #[test]
    fn test_channel_scope_all() {
        let scope = ChannelScope::All("all".into());
        assert!(scope.matches(&Uuid::new_v4()));
        assert!(scope.matches(&Uuid::new_v4()));
    }

    #[test]
    fn test_channel_scope_all_invalid_string() {
        // Only the literal "all" should match; other strings must not.
        let scope = ChannelScope::All("ALL".into());
        assert!(!scope.matches(&Uuid::new_v4()));

        let scope = ChannelScope::All("".into());
        assert!(!scope.matches(&Uuid::new_v4()));
    }

    #[test]
    fn test_channel_scope_list() {
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        let id_c = Uuid::new_v4();

        let scope = ChannelScope::List(vec![id_a.to_string(), id_b.to_string()]);

        assert!(scope.matches(&id_a));
        assert!(scope.matches(&id_b));
        assert!(!scope.matches(&id_c));
    }

    // ── prompt_tag fallback ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_prompt_tag_falls_back_to_name() {
        let event = make_event(40001, "hello");
        let channel_id = any_channel();

        let rules = vec![SubscriptionRule {
            name: "my-rule".into(),
            channels: ChannelScope::All("all".into()),
            kinds: vec![],
            require_mention: false,
            filter: None,
            prompt_tag: None, // no explicit tag
        }];

        let matched = match_event(&event, channel_id, &rules, "").await.unwrap();
        assert_eq!(matched.prompt_tag, "my-rule");
    }
}
