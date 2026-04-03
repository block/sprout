use evalexpr::{ContextWithMutableFunctions, Function, HashMapContext, Value};

/// Register shared string helper functions into an evalexpr context.
///
/// evalexpr v11 does not ship `str_contains`, `str_starts_with`,
/// `str_ends_with`, or `str_len`. This function registers them so both
/// sprout-acp (filter evaluation) and sprout-workflow (condition evaluation)
/// can use them without duplicating the registration code.
///
/// # Functions registered
///
/// - `str_contains(haystack, needle)` → bool
/// - `str_starts_with(s, prefix)` → bool
/// - `str_ends_with(s, suffix)` → bool
/// - `str_len(s)` → int
pub fn register_string_helpers(ctx: &mut HashMapContext) {
    ctx.set_function(
        "str_contains".into(),
        Function::new(|args| {
            let args = args.as_fixed_len_tuple(2)?;
            let haystack = args[0].as_string()?;
            let needle = args[1].as_string()?;
            Ok(Value::Boolean(haystack.contains(needle.as_str())))
        }),
    )
    .expect("str_contains registration cannot fail");

    ctx.set_function(
        "str_starts_with".into(),
        Function::new(|args| {
            let args = args.as_fixed_len_tuple(2)?;
            let s = args[0].as_string()?;
            let prefix = args[1].as_string()?;
            Ok(Value::Boolean(s.starts_with(prefix.as_str())))
        }),
    )
    .expect("str_starts_with registration cannot fail");

    ctx.set_function(
        "str_ends_with".into(),
        Function::new(|args| {
            let args = args.as_fixed_len_tuple(2)?;
            let s = args[0].as_string()?;
            let suffix = args[1].as_string()?;
            Ok(Value::Boolean(s.ends_with(suffix.as_str())))
        }),
    )
    .expect("str_ends_with registration cannot fail");

    ctx.set_function(
        "str_len".into(),
        Function::new(|arg| {
            let s = arg.as_string()?;
            Ok(Value::Int(s.len() as i64))
        }),
    )
    .expect("str_len registration cannot fail");
}
