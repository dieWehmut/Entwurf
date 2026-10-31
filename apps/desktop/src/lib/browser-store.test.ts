import "fake-indexeddb/auto";
import { describe, expect, it } from "vitest";
import { BrowserStore, StaleWriteError } from "./browser-store";

describe("persistent browser filesystem", () => {
  it("restores original bytes and project selection after reopening", async () => {
    const name = `test-${crypto.randomUUID()}`;
    const first = new BrowserStore(name);
    const before = await first.read();
    await first.commit(before.revision, new Map([["/projects/p/source.png", new Uint8Array([0, 127, 255])]]), { root: "/projects/p", taskId: "task-a" });
    const reopened = await new BrowserStore(name).read();
    expect(reopened.files.get("/projects/p/source.png")).toEqual(new Uint8Array([0, 127, 255]));
    expect(reopened.active).toEqual({ root: "/projects/p", taskId: "task-a" });
  });

  it("rejects a concurrent stale commit without damaging the winning files", async () => {
    const store = new BrowserStore(`test-${crypto.randomUUID()}`);
    const before = await store.read();
    await store.commit(before.revision, new Map([["/winner", new Uint8Array([42])]]), null);
    await expect(store.commit(before.revision, new Map([["/loser", new Uint8Array([99])]]), null)).rejects.toBeInstanceOf(StaleWriteError);
    const current = await store.read();
    expect([...current.files.keys()]).toEqual(["/winner"]);
    expect(current.files.get("/winner")).toEqual(new Uint8Array([42]));
  });

  it("atomically removes deleted files with the next revision", async () => {
    const store = new BrowserStore(`test-${crypto.randomUUID()}`);
    await store.commit(0, new Map([["/old", new Uint8Array([1])]]), null);
    await store.commit(1, new Map([["/new", new Uint8Array([2])]]), null);
    const current = await store.read();
    expect(current.revision).toBe(2);
    expect([...current.files.keys()]).toEqual(["/new"]);
  });
});
