import { describe, it, expect } from "vitest";
import { CATALOG_SCHEMA_VERSION, VERSION } from "./index.js";

describe("@telemetra/runtime public API", () => {
  it("targets catalog schema version 1", () => {
    expect(CATALOG_SCHEMA_VERSION).toBe(1);
  });

  it("exposes a non-empty version string", () => {
    expect(VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });
});
