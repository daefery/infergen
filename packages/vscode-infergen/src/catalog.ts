import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';
import * as vscode from 'vscode';

// ── Types (mirror infergen-types/src/lib.rs) ─────────────────────────────────

export type EventStatus = 'proposed' | 'approved' | 'ignored';
export type EventKind =
  | 'pageView'
  | 'apiCall'
  | 'authEvent'
  | 'formSubmit'
  | 'buttonClick'
  | 'search'
  | 'error';

export interface EventProvenance {
  sourcePath: string;
  line?: number;
  adapter: string;
}

export interface EventProperty {
  name: string;
  type?: string;
  required: boolean;
  pii: boolean;
}

export interface CatalogEntry {
  id: string;
  name: string;
  description: string;
  status: EventStatus;
  confidence: number;
  kind: EventKind;
  provenance: EventProvenance[];
  properties: EventProperty[];
  providers: string[];
  package?: string;
  flowIds?: string[];
}

export interface Catalog {
  schemaVersion: number;
  events: CatalogEntry[];
  flows?: unknown[];
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/** Parse catalog YAML string. Throws on invalid YAML or missing schemaVersion/events. */
export function parseCatalog(content: string): Catalog {
  const raw = yaml.load(content) as Record<string, unknown>;
  if (!raw || typeof raw !== 'object') {
    throw new Error('Invalid catalog: expected YAML object');
  }
  if (typeof raw['schemaVersion'] !== 'number') {
    throw new Error('Invalid catalog: missing schemaVersion');
  }
  if (!Array.isArray(raw['events'])) {
    throw new Error('Invalid catalog: missing events array');
  }
  return raw as unknown as Catalog;
}

/** Load catalog from disk. Returns null when file does not exist. Throws on parse errors. */
export function loadCatalog(catalogPath: string): Catalog | null {
  if (!fs.existsSync(catalogPath)) {
    return null;
  }
  const content = fs.readFileSync(catalogPath, 'utf-8');
  return parseCatalog(content);
}

// ── CatalogManager ────────────────────────────────────────────────────────────

/** Watches the catalog YAML file, caches the parsed result, and fires change events. */
export class CatalogManager implements vscode.Disposable {
  private catalog: Catalog | null = null;
  private readonly watcher: vscode.FileSystemWatcher;
  private readonly changeEmitter = new vscode.EventEmitter<Catalog | null>();

  /** Fires whenever the catalog is reloaded or becomes null (deleted). */
  readonly onDidChange = this.changeEmitter.event;

  constructor(private readonly catalogPath: string) {
    this.reload();

    const pattern = new vscode.RelativePattern(
      path.dirname(catalogPath),
      path.basename(catalogPath)
    );
    this.watcher = vscode.workspace.createFileSystemWatcher(pattern);
    this.watcher.onDidChange(() => this.reload());
    this.watcher.onDidCreate(() => this.reload());
    this.watcher.onDidDelete(() => {
      this.catalog = null;
      this.changeEmitter.fire(null);
    });
  }

  getCatalog(): Catalog | null {
    return this.catalog;
  }

  /** Returns only approved events (for autocomplete and hover). */
  getApprovedEvents(): CatalogEntry[] {
    return this.catalog?.events.filter((e) => e.status === 'approved') ?? [];
  }

  /** Returns approved + proposed events (excludes ignored). */
  getActiveEvents(): CatalogEntry[] {
    return this.catalog?.events.filter((e) => e.status !== 'ignored') ?? [];
  }

  private reload(): void {
    try {
      this.catalog = loadCatalog(this.catalogPath);
      this.changeEmitter.fire(this.catalog);
    } catch {
      this.catalog = null;
      this.changeEmitter.fire(null);
    }
  }

  dispose(): void {
    this.watcher.dispose();
    this.changeEmitter.dispose();
  }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/**
 * Convert snake_case event name to camelCase.
 * Mirrors infergen-core codegen `to_camel_case()`.
 */
export function toCamelCase(name: string): string {
  return name
    .split('_')
    .map((part, i) => (i === 0 ? part : part.charAt(0).toUpperCase() + part.slice(1)))
    .join('');
}

/** Check whether a word matches an event name (snake_case or camelCase). */
export function matchesEventName(word: string, eventName: string): boolean {
  return word === eventName || word === toCamelCase(eventName);
}
