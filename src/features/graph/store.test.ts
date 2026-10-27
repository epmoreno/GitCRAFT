import {
  commandError,
  ipc,
  sampleEntries,
  sampleRefs,
  streamEvents,
} from "../../test/ipc";
import { useGraphStore } from "./store";

const graph = (id: number) => useGraphStore.getState().graphs[id];

describe("graph store", () => {
  it("appends pages in order, tracks the widest column and loads refs", async () => {
    const entries = sampleEntries();
    ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
    streamEvents([
      { type: "page", entries: entries.slice(0, 2) },
      { type: "page", entries: entries.slice(2) },
      { type: "done", total: entries.length },
    ]);

    await useGraphStore.getState().load(1);

    expect(graph(1)?.status).toBe("done");
    expect(graph(1)?.entries.map((e) => e.commit.summary)).toEqual([
      "E",
      "Merge branch 'feature'",
      "D",
      "C",
      "B",
      "A",
    ]);
    expect(graph(1)?.maxColumn).toBe(1);
    expect(graph(1)?.refs?.refs).toHaveLength(4);
  });

  it("ignores events from a superseded load", async () => {
    const entries = sampleEntries();
    let staleSend:
      | ((e: Parameters<Parameters<typeof ipc.streamGraph>[1]>[0]) => void)
      | undefined;
    ipc.streamGraph.mockImplementationOnce(async (_id, onEvent) => {
      staleSend = onEvent;
      return { status: "error", error: commandError("cancelled") };
    });
    await useGraphStore.getState().load(1);

    streamEvents([{ type: "page", entries: entries.slice(0, 1) }]);
    await useGraphStore.getState().load(1);
    staleSend?.({ type: "page", entries: entries.slice(1) });

    expect(graph(1)?.entries).toHaveLength(1);
    expect(graph(1)?.error).toBeNull();
  });

  it("records load errors except cancellations", async () => {
    ipc.streamGraph.mockResolvedValue({
      status: "error",
      error: commandError("unknown", "$ git log\nfatal: bad object"),
    });
    await useGraphStore.getState().load(2);
    expect(graph(2)?.status).toBe("error");
    expect(graph(2)?.error?.details).toContain("bad object");
  });

  it("selects a commit and forgets a closed repository", async () => {
    streamEvents([{ type: "done", total: 0 }]);
    await useGraphStore.getState().load(3);
    useGraphStore.getState().select(3, "abc");
    expect(graph(3)?.selectedOid).toBe("abc");
    useGraphStore.getState().forget(3);
    expect(graph(3)).toBeUndefined();
  });
});
