import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { RepoRefs, TagInfo } from "../../ipc";
import { ipc, sampleRefs } from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { Sidebar } from "../sidebar/Sidebar";
import { TagDialogs, TagToast } from "./TagDialogs";
import { useTagStore } from "./tags";

const OID = "d00d".padEnd(40, "0");

function withRemotes(remotes: string[]) {
  const refs: RepoRefs = { ...sampleRefs(), remotes };
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs,
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

function renderTags() {
  render(
    <>
      <TagDialogs />
      <TagToast repoId={1} />
    </>,
  );
}

const info: TagInfo = {
  name: "v1.0",
  oid: "a".repeat(40),
  commit: "b".repeat(40),
  annotated: true,
};

beforeEach(async () => {
  await i18n.changeLanguage("es");
  ipc.checkTagName.mockImplementation((_id, name) =>
    Promise.resolve({
      status: "ok",
      data:
        name === "v1.0" ? "exists" : name.startsWith("-") ? "invalid" : "valid",
    }),
  );
  ipc.createTag.mockResolvedValue({ status: "ok", data: info });
  ipc.pushTag.mockResolvedValue({ status: "ok", data: { type: "done" } });
});

describe("creating a tag", () => {
  it("is annotated by default and needs a valid, new name and a message", async () => {
    const user = userEvent.setup();
    withRemotes(["origin"]);
    renderTags();
    await act(() => useTagStore.getState().requestCreate(1, OID));
    const dialog = screen.getByRole("alertdialog");
    const create = within(dialog).getByRole("button", { name: "Crear tag" });
    expect(
      within(dialog).getByRole("radio", { name: /Anotado/ }),
    ).toBeChecked();
    expect(create).toBeDisabled();

    const name = within(dialog).getByRole("textbox", { name: "Nombre" });
    await user.type(name, "v1.0");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Ya existe un tag con ese nombre",
    );
    await user.clear(name);
    await user.type(name, "v2.0");
    await user.type(
      within(dialog).getByRole("textbox", { name: "Mensaje" }),
      "Versión 2",
    );
    await vi.waitFor(() => expect(create).toBeEnabled());
    // Not pushed unless asked.
    expect(
      within(dialog).getByRole("checkbox", {
        name: "Subirlo también a «origin» después de crearlo",
      }),
    ).not.toBeChecked();
    await user.click(create);
    expect(ipc.createTag).toHaveBeenCalledWith(1, OID, "v2.0", "Versión 2");
    expect(ipc.pushTag).not.toHaveBeenCalled();
    expect(await screen.findByText("Tag «v2.0» creado.")).toBeInTheDocument();
  });

  it("creates a lightweight tag and pushes it to the chosen remote", async () => {
    const user = userEvent.setup();
    withRemotes(["origin", "backup"]);
    renderTags();
    await act(() => useTagStore.getState().requestCreate(1, OID));
    const dialog = screen.getByRole("alertdialog");
    await user.type(
      within(dialog).getByRole("textbox", { name: "Nombre" }),
      "light",
    );
    await user.click(within(dialog).getByRole("radio", { name: /Ligero/ }));
    expect(
      within(dialog).queryByRole("textbox", { name: "Mensaje" }),
    ).toBeNull();
    await user.click(within(dialog).getByRole("checkbox"));
    await user.selectOptions(
      within(dialog).getByRole("combobox", { name: "Remoto" }),
      "backup",
    );
    const create = within(dialog).getByRole("button", { name: "Crear tag" });
    await vi.waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    expect(ipc.createTag).toHaveBeenCalledWith(1, OID, "light", null);
    expect(ipc.pushTag).toHaveBeenCalledWith(1, "backup", "light");
    expect(
      await screen.findByText("Tag «light» creado y subido a «backup»."),
    ).toBeInTheDocument();
  });
});

describe("pushing and deleting", () => {
  it("never replaces a different tag on the remote", async () => {
    ipc.pushTag.mockResolvedValue({
      status: "ok",
      data: { type: "rejected", detail: "[rejected] (already exists)" },
    });
    renderTags();
    await act(() => useTagStore.getState().push(1, "v1.0", "origin"));
    expect(
      screen.getByText(/«origin» no ha aceptado el tag «v1.0»/),
    ).toBeInTheDocument();
  });

  it("deletes locally, says where it still is, and deletes there only from its own button", async () => {
    const user = userEvent.setup();
    ipc.tagInfo.mockResolvedValue({ status: "ok", data: info });
    ipc.remoteTags.mockResolvedValue({
      status: "ok",
      data: [
        { remote: "origin", oid: info.oid, same: true },
        { remote: "backup", oid: "c".repeat(40), same: false },
      ],
    });
    ipc.deleteTag.mockResolvedValue({ status: "ok", data: null });
    ipc.deleteRemoteTag.mockResolvedValue({
      status: "ok",
      data: { type: "done" },
    });
    renderTags();
    await act(() => useTagStore.getState().requestDelete(1, "v1.0"));
    const dialog = screen.getByRole("alertdialog", {
      name: "¿Borrar el tag «v1.0»?",
    });
    expect(dialog).toHaveTextContent("Se borra solo en tu copia.");
    expect(dialog).toHaveTextContent(
      "También está en origin: allí no se borra.",
    );
    expect(dialog).toHaveTextContent("En backup hay otro tag distinto");
    await user.click(
      within(dialog).getByRole("button", { name: "Borrar tag" }),
    );
    expect(ipc.deleteTag).toHaveBeenCalledWith(1, "v1.0", info.oid);
    expect(ipc.deleteRemoteTag).not.toHaveBeenCalled();
    expect(
      await screen.findByText("Tag «v1.0» borrado en local. Sigue en origin."),
    ).toBeInTheDocument();
    // Only the remote with the same tag is offered.
    expect(screen.queryByRole("button", { name: /backup/ })).toBeNull();

    await user.click(
      screen.getByRole("button", { name: "Borrarlo también de «origin»…" }),
    );
    const remote = screen.getByRole("alertdialog", {
      name: "¿Borrar el tag «v1.0» de «origin»?",
    });
    expect(remote).toHaveTextContent("Afecta a otras personas");
    expect(remote).toHaveTextContent(
      "si alguien lo vuelve a subir, reaparecerá",
    );
    expect(ipc.deleteRemoteTag).not.toHaveBeenCalled();
    await user.click(
      within(remote).getByRole("button", { name: "Borrar de «origin»" }),
    );
    expect(ipc.deleteRemoteTag).toHaveBeenCalledWith(
      1,
      "origin",
      "v1.0",
      info.oid,
    );
    expect(
      await screen.findByText("Tag «v1.0» borrado de «origin»."),
    ).toBeInTheDocument();
  });

  it("has push, copy and delete in the sidebar's tag menu", async () => {
    withRemotes(["origin"]);
    ipc.getRefs.mockResolvedValue({
      status: "ok",
      data: { ...sampleRefs(), remotes: ["origin"] },
    });
    render(<Sidebar repoId={1} />);
    fireEvent.contextMenu(await screen.findByRole("button", { name: "v1.0" }));
    const menu = screen.getByRole("menu");
    const items = within(menu)
      .getAllByRole("menuitem")
      .map((i) => i.textContent);
    expect(items).toEqual(
      expect.arrayContaining([
        "Subir a «origin»",
        "Copiar nombre",
        "Borrar tag…",
      ]),
    );
  });
});
