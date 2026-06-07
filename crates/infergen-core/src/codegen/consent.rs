//! Consent & PII Controls codegen (E3.4).
//!
//! Emits consent gating, opt-out, per-property redaction hooks, and region
//! routing into every generated SDK file. All track functions call `_dispatch`
//! (emitted here) instead of iterating `_providers` directly.

pub fn write_consent_module(out: &mut String) {
    out.push_str("\n// ─── Consent & PII Controls ");
    out.push_str(&"─".repeat(53));
    out.push('\n');
    out.push('\n');

    // ConsentState type
    out.push_str("/** Consent state for analytics event dispatch. */\n");
    out.push_str("export type ConsentState = \"granted\" | \"denied\" | \"unknown\";\n");
    out.push('\n');

    // RedactFn type
    out.push_str("/**\n");
    out.push_str(" * A function that transforms or redacts a single event property before dispatch.\n");
    out.push_str(" * Return the (possibly transformed) value to include the property.\n");
    out.push_str(" * Return `null` to drop the property entirely from the dispatched payload.\n");
    out.push_str(" */\n");
    out.push_str("export type RedactFn = (\n");
    out.push_str("  propertyName: string,\n");
    out.push_str("  value: unknown,\n");
    out.push_str("  eventName: string,\n");
    out.push_str(") => unknown | null;\n");
    out.push('\n');

    // Internal state variables
    out.push_str("/** @internal Current consent state — defaults to \"unknown\" (permissive until explicitly set). */\n");
    out.push_str("let _consentState: ConsentState = \"unknown\";\n");
    out.push_str("/** @internal Whether the user has globally opted out of all tracking. */\n");
    out.push_str("let _optedOut = false;\n");
    out.push_str("/** @internal Property-level redaction hook registered via setRedactFn. */\n");
    out.push_str("let _redactFn: RedactFn | null = null;\n");
    out.push_str("/** @internal Active region tag for provider routing. */\n");
    out.push_str("let _region: string | null = null;\n");
    out.push_str("/** @internal Region-to-provider-id routing table. */\n");
    out.push_str("let _regionRoutes: Record<string, string[]> = {};\n");
    out.push('\n');

    // setConsent / getConsent
    out.push_str("/** Set the current consent state. \"denied\" blocks all event dispatch. */\n");
    out.push_str("export function setConsent(state: ConsentState): void {\n");
    out.push_str("  _consentState = state;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("/** Get the current consent state. */\n");
    out.push_str("export function getConsent(): ConsentState {\n");
    out.push_str("  return _consentState;\n");
    out.push_str("}\n");
    out.push('\n');

    // optOut / optIn / isOptedOut
    out.push_str("/** Globally opt out of all analytics tracking. No events are dispatched until optIn() is called. */\n");
    out.push_str("export function optOut(): void {\n");
    out.push_str("  _optedOut = true;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("/** Opt back in to analytics tracking. */\n");
    out.push_str("export function optIn(): void {\n");
    out.push_str("  _optedOut = false;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("/** Returns true when the user has opted out of all analytics. */\n");
    out.push_str("export function isOptedOut(): boolean {\n");
    out.push_str("  return _optedOut;\n");
    out.push_str("}\n");
    out.push('\n');

    // setRedactFn
    out.push_str("/** Register a per-property redaction hook, or pass null to clear. */\n");
    out.push_str("export function setRedactFn(fn: RedactFn | null): void {\n");
    out.push_str("  _redactFn = fn;\n");
    out.push_str("}\n");
    out.push('\n');

    // setRegion / getRegion
    out.push_str("/** Set the active region tag (e.g. \"eu\", \"us\"). Used for provider routing. */\n");
    out.push_str("export function setRegion(region: string | null): void {\n");
    out.push_str("  _region = region;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("/** Get the active region tag. */\n");
    out.push_str("export function getRegion(): string | null {\n");
    out.push_str("  return _region;\n");
    out.push_str("}\n");
    out.push('\n');

    // setRegionRoutes
    out.push_str("/**\n");
    out.push_str(" * Configure region-based provider routing.\n");
    out.push_str(" * Keys are region tags; values are arrays of provider ids that should receive\n");
    out.push_str(" * events when that region is active. Providers not in the list are skipped.\n");
    out.push_str(" *\n");
    out.push_str(" * @example\n");
    out.push_str(" * setRegionRoutes({ eu: [\"posthog-eu\"], us: [\"posthog-us\", \"amplitude\"] });\n");
    out.push_str(" */\n");
    out.push_str("export function setRegionRoutes(routes: Record<string, string[]>): void {\n");
    out.push_str("  _regionRoutes = routes;\n");
    out.push_str("}\n");
    out.push('\n');

    // _dispatch
    out.push_str("/**\n");
    out.push_str(" * @internal\n");
    out.push_str(" * Central dispatch — applies consent gate, opt-out, PII redaction, and region\n");
    out.push_str(" * routing before forwarding to providers. All generated track functions call this.\n");
    out.push_str(" */\n");
    out.push_str("export function _dispatch(\n");
    out.push_str("  eventName: string,\n");
    out.push_str("  properties: Record<string, unknown>,\n");
    out.push_str("): void {\n");
    out.push_str("  // Opt-out and consent gate\n");
    out.push_str("  if (_optedOut || _consentState === \"denied\") return;\n");
    out.push('\n');
    out.push_str("  // PII redaction\n");
    out.push_str("  let payload: Record<string, unknown> = properties;\n");
    out.push_str("  if (_redactFn !== null) {\n");
    out.push_str("    payload = {};\n");
    out.push_str("    for (const [key, value] of Object.entries(properties)) {\n");
    out.push_str("      const result = _redactFn(key, value, eventName);\n");
    out.push_str("      if (result !== null) payload[key] = result;\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  // Region routing: filter providers to those allowed for the current region\n");
    out.push_str("  let targets = _providers;\n");
    out.push_str("  if (_region !== null && _regionRoutes[_region] !== undefined) {\n");
    out.push_str("    const allowed = _regionRoutes[_region];\n");
    out.push_str("    targets = _providers.filter(p => allowed.includes(p.id));\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  targets.forEach(p => p.track(eventName, payload));\n");
    out.push_str("}\n");
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> String {
        let mut s = String::new();
        write_consent_module(&mut s);
        s
    }

    #[test]
    fn consent_section_divider() {
        assert!(output().contains("Consent & PII Controls"), "section header missing");
    }

    #[test]
    fn consent_state_type_exported() {
        assert!(output().contains("export type ConsentState"), "ConsentState missing");
    }

    #[test]
    fn consent_state_has_three_values() {
        let o = output();
        assert!(o.contains("\"granted\""), "granted missing");
        assert!(o.contains("\"denied\""), "denied missing");
        assert!(o.contains("\"unknown\""), "unknown missing");
    }

    #[test]
    fn redact_fn_type_exported() {
        assert!(output().contains("export type RedactFn"), "RedactFn missing");
    }

    #[test]
    fn set_get_consent_exported() {
        let o = output();
        assert!(o.contains("export function setConsent"), "setConsent missing");
        assert!(o.contains("export function getConsent"), "getConsent missing");
    }

    #[test]
    fn opt_out_in_exported() {
        let o = output();
        assert!(o.contains("export function optOut"), "optOut missing");
        assert!(o.contains("export function optIn"), "optIn missing");
    }

    #[test]
    fn is_opted_out_exported() {
        assert!(output().contains("export function isOptedOut"), "isOptedOut missing");
    }

    #[test]
    fn set_redact_fn_exported() {
        assert!(output().contains("export function setRedactFn"), "setRedactFn missing");
    }

    #[test]
    fn set_get_region_exported() {
        let o = output();
        assert!(o.contains("export function setRegion"), "setRegion missing");
        assert!(o.contains("export function getRegion"), "getRegion missing");
    }

    #[test]
    fn set_region_routes_exported() {
        assert!(output().contains("export function setRegionRoutes"), "setRegionRoutes missing");
    }

    #[test]
    fn dispatch_function_present() {
        assert!(output().contains("export function _dispatch"), "_dispatch missing");
    }

    #[test]
    fn dispatch_blocks_on_opted_out() {
        assert!(output().contains("if (_optedOut"), "opt-out check missing");
    }

    #[test]
    fn dispatch_blocks_on_denied() {
        assert!(
            output().contains("_consentState === \"denied\""),
            "denied consent check missing"
        );
    }

    #[test]
    fn dispatch_applies_redact_fn() {
        assert!(
            output().contains("_redactFn(key, value, eventName)"),
            "redactFn call missing"
        );
    }

    #[test]
    fn dispatch_drops_null_redacted() {
        assert!(output().contains("if (result !== null)"), "null-drop check missing");
    }

    #[test]
    fn dispatch_region_routing() {
        let o = output();
        assert!(o.contains("_regionRoutes[_region]"), "region lookup missing");
        assert!(o.contains("allowed.includes(p.id)"), "id filter missing");
    }

    #[test]
    fn dispatch_delegates_to_targets() {
        assert!(
            output().contains("targets.forEach(p => p.track(eventName"),
            "targets dispatch missing"
        );
    }

    #[test]
    fn consent_state_defaults_unknown() {
        assert!(
            output().contains("_consentState: ConsentState = \"unknown\""),
            "default unknown missing"
        );
    }

    #[test]
    fn opted_out_defaults_false() {
        assert!(output().contains("_optedOut = false"), "default false missing");
    }
}
