import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { resolveConfig } from '../config';

let tmpDir: string;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infergen-cfg-'));
});

afterEach(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

describe('resolveConfig', () => {
  it('returns default catalog path when no config file exists', () => {
    const { catalogPath } = resolveConfig(tmpDir);
    expect(catalogPath).toBe(path.join(tmpDir, '.infergen', 'catalog.yaml'));
  });

  it('reads catalog path from infergen.config.json', () => {
    fs.writeFileSync(
      path.join(tmpDir, 'infergen.config.json'),
      JSON.stringify({ catalog: 'custom/catalog.yaml' })
    );
    const { catalogPath } = resolveConfig(tmpDir);
    expect(catalogPath).toBe(path.join(tmpDir, 'custom', 'catalog.yaml'));
  });

  it('uses default when catalog field missing from JSON', () => {
    fs.writeFileSync(
      path.join(tmpDir, 'infergen.config.json'),
      JSON.stringify({ output: 'generated/' })
    );
    const { catalogPath } = resolveConfig(tmpDir);
    expect(catalogPath).toBe(path.join(tmpDir, '.infergen', 'catalog.yaml'));
  });

  it('uses default when JSON is invalid', () => {
    fs.writeFileSync(path.join(tmpDir, 'infergen.config.json'), 'not json {{{');
    const { catalogPath } = resolveConfig(tmpDir);
    expect(catalogPath).toBe(path.join(tmpDir, '.infergen', 'catalog.yaml'));
  });

  it('resolves absolute catalog path as-is', () => {
    const abs = path.join(os.tmpdir(), 'my-catalog.yaml');
    fs.writeFileSync(
      path.join(tmpDir, 'infergen.config.json'),
      JSON.stringify({ catalog: abs })
    );
    const { catalogPath } = resolveConfig(tmpDir);
    expect(catalogPath).toBe(abs);
  });

  it('resolves relative parent path correctly', () => {
    fs.writeFileSync(
      path.join(tmpDir, 'infergen.config.json'),
      JSON.stringify({ catalog: '../shared/catalog.yaml' })
    );
    const { catalogPath } = resolveConfig(tmpDir);
    expect(catalogPath).toBe(path.resolve(tmpDir, '../shared/catalog.yaml'));
  });

  it('uses default when catalog field is empty string', () => {
    fs.writeFileSync(
      path.join(tmpDir, 'infergen.config.json'),
      JSON.stringify({ catalog: '' })
    );
    const { catalogPath } = resolveConfig(tmpDir);
    expect(catalogPath).toBe(path.join(tmpDir, '.infergen', 'catalog.yaml'));
  });
});
