import { describe, expect, it } from "vitest";
import { mockAdapter } from "../../src/lib/ipc/mock";

describe("settings adapter (T14 demo surface)", () => {
  it("returns default settings v1", async () => {
    const settings = await mockAdapter.settingsGet();
    expect(settings.version).toBeGreaterThan(0);
    expect(settings.fontScale).toBe(1);
  });

  it("updates the font scale with version match and bumps the version", async () => {
    const before = await mockAdapter.settingsGet();
    const next = await mockAdapter.settingsUpdate(before.version, 1.125);
    expect(next.fontScale).toBe(1.125);
    expect(next.version).toBe(before.version + 1);
  });

  it("rejects stale versions and out-of-range scales", async () => {
    const current = await mockAdapter.settingsGet();
    await expect(mockAdapter.settingsUpdate(current.version - 1, 1.0)).rejects.toMatchObject({
      code: "STALE_STATE"
    });
    await expect(mockAdapter.settingsUpdate(current.version, 2.0)).rejects.toMatchObject({
      code: "INVALID_ARGUMENT"
    });
  });
});
