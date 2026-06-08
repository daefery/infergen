import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { parseCatalog, loadCatalog, toCamelCase, matchesEventName } from '../catalog';

const MINIMAL_CATALOG = `
schemaVersion: 1
events: []
`;

const FULL_ENTRY_CATALOG = `
schemaVersion: 1
events:
  - id: evt_abc
    name: user_signed_in
    description: "User auth success"
    status: approved
    confidence: 0.9
    kind: authEvent
    provenance:
      - sourcePath: src/auth.ts
        line: 42
        adapter: nextjs
    properties:
      - name: email
        type: string
        required: true
        pii: true
    providers: []
`;

describe('parseCatalog', () => {
  it('parses minimal valid YAML', () => {
    const c = parseCatalog(MINIMAL_CATALOG);
    expect(c.schemaVersion).toBe(1);
    expect(c.events).toEqual([]);
  });

  it('throws on invalid YAML', () => {
    expect(() => parseCatalog('!!!')).toThrow();
  });

  it('throws when schemaVersion missing', () => {
    expect(() => parseCatalog('events: []')).toThrow(/schemaVersion/);
  });

  it('throws when events not array', () => {
    expect(() => parseCatalog('schemaVersion: 1\nevents: null')).toThrow(/events/);
  });

  it('parses full entry fields', () => {
    const c = parseCatalog(FULL_ENTRY_CATALOG);
    const e = c.events[0];
    expect(e.id).toBe('evt_abc');
    expect(e.name).toBe('user_signed_in');
    expect(e.status).toBe('approved');
    expect(e.provenance[0].sourcePath).toBe('src/auth.ts');
    expect(e.provenance[0].line).toBe(42);
    expect(e.properties[0].pii).toBe(true);
    expect(e.properties[0].required).toBe(true);
  });

  it('parses empty events array', () => {
    const c = parseCatalog('schemaVersion: 1\nevents: []');
    expect(c.events).toHaveLength(0);
  });
});

describe('loadCatalog', () => {
  it('returns null for non-existent path', () => {
    expect(loadCatalog('/does/not/exist/catalog.yaml')).toBeNull();
  });

  let tmp: string;
  beforeEach(() => { tmp = ''; });
  afterEach(() => { if (tmp) fs.unlinkSync(tmp); });

  it('loads and parses a real file', () => {
    tmp = path.join(os.tmpdir(), `catalog_${Date.now()}.yaml`);
    fs.writeFileSync(tmp, MINIMAL_CATALOG);
    const c = loadCatalog(tmp);
    expect(c?.schemaVersion).toBe(1);
  });
});

describe('toCamelCase', () => {
  it('leaves single word unchanged', () => expect(toCamelCase('page')).toBe('page'));
  it('converts two-word name', () => expect(toCamelCase('user_signed_in')).toBe('userSignedIn'));
  it('converts three-word name', () =>
    expect(toCamelCase('checkout_step_completed')).toBe('checkoutStepCompleted'));
  it('handles already-camel input (no underscores)', () =>
    expect(toCamelCase('page')).toBe('page'));
});

describe('matchesEventName', () => {
  it('matches direct snake_case', () =>
    expect(matchesEventName('user_signed_in', 'user_signed_in')).toBe(true));
  it('matches camelCase variant', () =>
    expect(matchesEventName('userSignedIn', 'user_signed_in')).toBe(true));
  it('does not match unrelated word', () =>
    expect(matchesEventName('foo', 'bar')).toBe(false));
  it('does not match partial prefix', () =>
    expect(matchesEventName('user', 'user_signed_in')).toBe(false));
});
