//! TypeScript delivery-engine codegen (E3.3).
//!
//! Emits `DeliveryOptions`, `DeliveryEngine`, and `withDelivery` into every
//! generated SDK file. The engine wraps any `Provider` with batching, retry,
//! sampling, and flush-on-exit — zero npm dependencies.

pub fn write_delivery_engine(out: &mut String) {
    out.push_str("\n// ─── Delivery Engine ");
    out.push_str(&"─".repeat(60));
    out.push('\n');
    out.push('\n');

    // DeliveryOptions interface
    out.push_str("/** Options for DeliveryEngine. */\n");
    out.push_str("export interface DeliveryOptions {\n");
    out.push_str("  /** Number of events to collect before an automatic flush. Default: 10. */\n");
    out.push_str("  batchSize?: number;\n");
    out.push_str("  /** Milliseconds between timer-driven flushes. 0 disables the timer. Default: 5000. */\n");
    out.push_str("  flushInterval?: number;\n");
    out.push_str("  /** Maximum delivery attempts per event before it is dropped. Default: 3. */\n");
    out.push_str("  maxRetries?: number;\n");
    out.push_str("  /** Base retry delay in milliseconds; doubles each attempt (exponential backoff). Default: 1000. */\n");
    out.push_str("  retryDelay?: number;\n");
    out.push_str("  /** Fraction of events to forward (0.0 = drop all, 1.0 = forward all). Default: 1.0. */\n");
    out.push_str("  sampleRate?: number;\n");
    out.push_str("  /** Maximum number of queued events. Oldest is dropped when the queue is full. Default: 1000. */\n");
    out.push_str("  maxQueueSize?: number;\n");
    out.push_str("  /**\n");
    out.push_str("   * Persistence key for the event queue.\n");
    out.push_str("   * Browser: key in localStorage. Omit for in-memory queue only.\n");
    out.push_str("   */\n");
    out.push_str("  persistenceKey?: string;\n");
    out.push_str("}\n");
    out.push('\n');

    // _QueuedEvent internal interface
    out.push_str("/** @internal Queued event payload — compact keys keep storage small. */\n");
    out.push_str("interface _QueuedEvent {\n");
    out.push_str("  n: string;\n");
    out.push_str("  p: Record<string, unknown>;\n");
    out.push_str("}\n");
    out.push('\n');

    // DeliveryEngine class
    out.push_str("/**\n");
    out.push_str(" * Wraps a {@link Provider} with batching, retry, sampling, and flush-on-exit.\n");
    out.push_str(" *\n");
    out.push_str(" * @example\n");
    out.push_str(" * ```ts\n");
    out.push_str(" * configureInfergen({\n");
    out.push_str(" *   providers: [withDelivery(new PostHogProvider({ apiKey: \"phc_...\" }))],\n");
    out.push_str(" * });\n");
    out.push_str(" * ```\n");
    out.push_str(" */\n");
    out.push_str("export class DeliveryEngine implements Provider {\n");
    out.push_str("  readonly id: string;\n");
    out.push_str("  private _q: _QueuedEvent[] = [];\n");
    out.push_str("  private _timer: ReturnType<typeof setInterval> | null = null;\n");
    out.push_str("  private _flushing = false;\n");
    out.push_str("  private readonly _o: Required<Omit<DeliveryOptions, \"persistenceKey\">> & { persistenceKey?: string };\n");
    out.push('\n');
    out.push_str("  constructor(private readonly _inner: Provider, opts: DeliveryOptions = {}) {\n");
    out.push_str("    this.id = `delivery:${_inner.id}`;\n");
    out.push_str("    this._o = {\n");
    out.push_str("      batchSize:     opts.batchSize     ?? 10,\n");
    out.push_str("      flushInterval: opts.flushInterval ?? 5000,\n");
    out.push_str("      maxRetries:    opts.maxRetries    ?? 3,\n");
    out.push_str("      retryDelay:    opts.retryDelay    ?? 1000,\n");
    out.push_str("      sampleRate:    opts.sampleRate    ?? 1.0,\n");
    out.push_str("      maxQueueSize:  opts.maxQueueSize  ?? 1000,\n");
    out.push_str("      persistenceKey: opts.persistenceKey,\n");
    out.push_str("    };\n");
    out.push_str("    this._load();\n");
    out.push_str("    this._startTimer();\n");
    out.push_str("    this._exitHook();\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  track(eventName: string, properties: Record<string, unknown>): void {\n");
    out.push_str("    if (this._o.sampleRate < 1.0 && Math.random() > this._o.sampleRate) return;\n");
    out.push_str("    if (this._q.length >= this._o.maxQueueSize) this._q.shift();\n");
    out.push_str("    this._q.push({ n: eventName, p: properties });\n");
    out.push_str("    this._save();\n");
    out.push_str("    if (this._q.length >= this._o.batchSize) void this._flush();\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  flush(): void { void this._flush(); }\n");
    out.push('\n');
    out.push_str("  shutdown(): void { this._stopTimer(); void this._flush(); }\n");
    out.push('\n');
    out.push_str("  private _startTimer(): void {\n");
    out.push_str("    if (this._o.flushInterval > 0) {\n");
    out.push_str("      this._timer = setInterval(() => void this._flush(), this._o.flushInterval);\n");
    out.push_str("      const t = this._timer as unknown as { unref?: () => void };\n");
    out.push_str("      if (typeof t.unref === \"function\") t.unref();\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  private _stopTimer(): void {\n");
    out.push_str("    if (this._timer !== null) { clearInterval(this._timer); this._timer = null; }\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  private async _flush(): Promise<void> {\n");
    out.push_str("    if (this._flushing || this._q.length === 0) return;\n");
    out.push_str("    this._flushing = true;\n");
    out.push_str("    try {\n");
    out.push_str("      const batch = this._q.splice(0, this._o.batchSize);\n");
    out.push_str("      this._save();\n");
    out.push_str("      await Promise.all(batch.map(e => this._deliver(e)));\n");
    out.push_str("    } finally {\n");
    out.push_str("      this._flushing = false;\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  private async _deliver(e: _QueuedEvent): Promise<void> {\n");
    out.push_str("    for (let i = 0; i <= this._o.maxRetries; i++) {\n");
    out.push_str("      try { this._inner.track(e.n, e.p); return; } catch { /* retry */ }\n");
    out.push_str("      if (i < this._o.maxRetries)\n");
    out.push_str("        await new Promise<void>(r => setTimeout(r, this._o.retryDelay * (2 ** i)));\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  private _save(): void {\n");
    out.push_str("    const key = this._o.persistenceKey;\n");
    out.push_str("    if (!key) return;\n");
    out.push_str("    try {\n");
    out.push_str("      if (typeof localStorage !== \"undefined\") localStorage.setItem(key, JSON.stringify(this._q));\n");
    out.push_str("    } catch { /* storage quota or unavailable */ }\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  private _load(): void {\n");
    out.push_str("    const key = this._o.persistenceKey;\n");
    out.push_str("    if (!key) return;\n");
    out.push_str("    try {\n");
    out.push_str("      if (typeof localStorage !== \"undefined\") {\n");
    out.push_str("        const raw = localStorage.getItem(key);\n");
    out.push_str("        if (raw) this._q = JSON.parse(raw) as _QueuedEvent[];\n");
    out.push_str("      }\n");
    out.push_str("    } catch { /* ignore corrupt data */ }\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  private _exitHook(): void {\n");
    out.push_str("    if (typeof process !== \"undefined\" && typeof process.on === \"function\") {\n");
    out.push_str("      process.on(\"beforeExit\", () => this.shutdown());\n");
    out.push_str("    } else if (typeof window !== \"undefined\") {\n");
    out.push_str("      window.addEventListener(\"visibilitychange\", () => {\n");
    out.push_str("        if (document.visibilityState === \"hidden\") this.flush();\n");
    out.push_str("      });\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push_str("}\n");
    out.push('\n');

    // withDelivery helper
    out.push_str("/** Wrap a provider in a {@link DeliveryEngine} with optional delivery options. */\n");
    out.push_str("export function withDelivery(provider: Provider, opts?: DeliveryOptions): DeliveryEngine {\n");
    out.push_str("  return new DeliveryEngine(provider, opts);\n");
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
        write_delivery_engine(&mut s);
        s
    }

    #[test]
    fn delivery_engine_section_divider() {
        assert!(output().contains("Delivery Engine"), "section header missing");
    }

    #[test]
    fn delivery_options_interface_present() {
        assert!(output().contains("export interface DeliveryOptions"), "DeliveryOptions missing");
    }

    #[test]
    fn delivery_engine_class_present() {
        assert!(output().contains("export class DeliveryEngine"), "DeliveryEngine class missing");
    }

    #[test]
    fn delivery_engine_implements_provider() {
        assert!(output().contains("implements Provider"), "implements Provider missing");
    }

    #[test]
    fn delivery_engine_has_batch_size() {
        assert!(output().contains("batchSize"), "batchSize missing");
    }

    #[test]
    fn delivery_engine_has_flush_interval() {
        let o = output();
        assert!(o.contains("flushInterval"), "flushInterval missing");
        assert!(o.contains("setInterval"), "setInterval missing");
    }

    #[test]
    fn delivery_engine_has_retry() {
        let o = output();
        assert!(o.contains("maxRetries"), "maxRetries missing");
        assert!(o.contains("retryDelay"), "retryDelay missing");
    }

    #[test]
    fn delivery_engine_has_sampling() {
        let o = output();
        assert!(o.contains("sampleRate"), "sampleRate missing");
        assert!(o.contains("Math.random()"), "Math.random() missing");
    }

    #[test]
    fn delivery_engine_has_max_queue_size() {
        assert!(output().contains("maxQueueSize"), "maxQueueSize missing");
    }

    #[test]
    fn delivery_engine_has_persistence() {
        let o = output();
        assert!(o.contains("persistenceKey"), "persistenceKey missing");
        assert!(o.contains("localStorage"), "localStorage missing");
    }

    #[test]
    fn delivery_engine_has_exit_hook_node() {
        assert!(output().contains("beforeExit"), "beforeExit hook missing");
    }

    #[test]
    fn delivery_engine_has_exit_hook_browser() {
        assert!(output().contains("visibilitychange"), "visibilitychange hook missing");
    }

    #[test]
    fn delivery_engine_has_with_delivery() {
        assert!(output().contains("export function withDelivery"), "withDelivery missing");
    }

    #[test]
    fn delivery_engine_id_prefixed() {
        assert!(output().contains("delivery:${_inner.id}"), "delivery: id prefix missing");
    }

    #[test]
    fn delivery_engine_flush_method() {
        assert!(output().contains("flush(): void"), "flush() method missing");
    }

    #[test]
    fn delivery_engine_shutdown_method() {
        assert!(output().contains("shutdown(): void"), "shutdown() method missing");
    }

    #[test]
    fn delivery_engine_unref_guard() {
        assert!(output().contains(r#"typeof t.unref === "function""#), "unref guard missing");
    }

    #[test]
    fn delivery_engine_exponential_backoff() {
        assert!(output().contains("2 ** i"), "exponential backoff missing");
    }

    #[test]
    fn delivery_engine_concurrent_flush() {
        assert!(output().contains("Promise.all"), "Promise.all missing");
    }

    #[test]
    fn delivery_engine_flushing_guard() {
        assert!(output().contains("_flushing"), "_flushing guard missing");
    }
}
