import { describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { MAX_TOASTS, dismissToast, pushToast, resetToastsForTests, toasts } from "../../src/lib/toast";

describe("toast store", () => {
  it("stacks toasts with unique ids", () => {
    resetToastsForTests();
    const first = pushToast("success", "Pushed", 0);
    const second = pushToast("info", "Pushing…", 0);
    expect(first).not.toBe(second);
    expect(get(toasts)).toEqual([
      { id: first, kind: "success", message: "Pushed" },
      { id: second, kind: "info", message: "Pushing…" }
    ]);
  });

  it("caps the stack by dropping the oldest", () => {
    resetToastsForTests();
    for (let i = 0; i < MAX_TOASTS + 2; i++) pushToast("success", `done ${i}`, 0);
    const items = get(toasts);
    expect(items).toHaveLength(MAX_TOASTS);
    expect(items[0]?.message).toBe("done 2");
  });

  it("dismisses one toast by id", () => {
    resetToastsForTests();
    const keep = pushToast("success", "keep", 0);
    const drop = pushToast("success", "drop", 0);
    dismissToast(drop);
    expect(get(toasts).map((toast) => toast.id)).toEqual([keep]);
  });

  it("auto-dismisses after the ttl", () => {
    resetToastsForTests();
    vi.useFakeTimers();
    try {
      pushToast("info", "Pulling…", 1000);
      expect(get(toasts)).toHaveLength(1);
      vi.advanceTimersByTime(1000);
      expect(get(toasts)).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });
});
