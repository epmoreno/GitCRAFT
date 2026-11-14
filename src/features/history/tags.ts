import { create } from "zustand";
import {
  commands,
  type CommandError,
  type RemoteTag,
  type TagInfo,
} from "../../ipc";
import { useGraphStore } from "../graph/store";
import { refreshAfterRemote } from "../remote/store";

/**
 * Tags (task 10.5): create on a commit (annotated by default, optionally
 * pushed), push, delete locally, and — only from its own button, with its
 * own confirmation — delete on a remote.
 */

export type TagDialog =
  | {
      kind: "create";
      repoId: number;
      oid: string;
      short: string;
      subject: string;
    }
  | {
      kind: "delete";
      repoId: number;
      info: TagInfo;
      /** `null` while the remotes are being asked. */
      remotes: RemoteTag[] | null;
      remotesError: CommandError | null;
    }
  | { kind: "deleteRemote"; repoId: number; name: string; tag: RemoteTag };

export type TagNotice =
  | { kind: "created"; repoId: number; name: string; pushedTo: string | null }
  | { kind: "pushed"; repoId: number; name: string; remote: string }
  | {
      kind: "rejected";
      repoId: number;
      name: string;
      remote: string;
      detail: string;
    }
  | { kind: "deleted"; repoId: number; name: string; stillOn: RemoteTag[] }
  | { kind: "deletedRemote"; repoId: number; name: string; remote: string }
  | { kind: "error"; repoId: number; error: CommandError };

export type CreateRequest = {
  name: string;
  /** `null`: lightweight. */
  message: string | null;
  /** Push to this remote afterwards. */
  pushTo: string | null;
};

type TagState = {
  dialog: TagDialog | null;
  notice: TagNotice | null;
  busy: boolean;
  requestCreate: (repoId: number, oid: string) => Promise<void>;
  create: (request: CreateRequest) => Promise<void>;
  push: (repoId: number, name: string, remote: string) => Promise<void>;
  requestDelete: (repoId: number, name: string) => Promise<void>;
  confirmDelete: () => Promise<void>;
  requestDeleteRemote: (repoId: number, name: string, tag: RemoteTag) => void;
  confirmDeleteRemote: () => Promise<void>;
  closeDialog: () => void;
  dismissNotice: () => void;
};

export const initialTagState = { dialog: null, notice: null, busy: false };

export const useTagStore = create<TagState>()((set, get) => {
  const fail = (repoId: number, error: CommandError) =>
    set({ dialog: null, notice: { kind: "error", repoId, error } });

  const pushNow = async (repoId: number, name: string, remote: string) => {
    const result = await commands.pushTag(repoId, remote, name);
    if (result.status === "error") {
      fail(repoId, result.error);
      return false;
    }
    if (result.data.type === "rejected") {
      set({
        notice: {
          kind: "rejected",
          repoId,
          name,
          remote,
          detail: result.data.detail,
        },
      });
      return false;
    }
    return true;
  };

  return {
    ...initialTagState,

    requestCreate: async (repoId, oid) => {
      set({ notice: null });
      const line = useGraphStore
        .getState()
        .graphs[repoId]?.lines.find((l) => l.oid === oid);
      set({
        dialog: {
          kind: "create",
          repoId,
          oid,
          short: oid.slice(0, 7),
          subject: line?.summary ?? "",
        },
      });
    },

    create: async ({ name, message, pushTo }) => {
      const dialog = get().dialog;
      if (dialog?.kind !== "create") return;
      const { repoId, oid } = dialog;
      set({ busy: true });
      const result = await commands.createTag(repoId, oid, name, message);
      if (result.status === "error") {
        set({ busy: false });
        fail(repoId, result.error);
        return;
      }
      set({ dialog: null });
      const pushed = pushTo ? await pushNow(repoId, name, pushTo) : false;
      set({ busy: false });
      refreshAfterRemote(repoId);
      if (!pushTo || pushed) {
        set({
          notice: {
            kind: "created",
            repoId,
            name,
            pushedTo: pushed ? pushTo : null,
          },
        });
      }
    },

    push: async (repoId, name, remote) => {
      set({ notice: null, busy: true });
      const ok = await pushNow(repoId, name, remote);
      set({ busy: false });
      refreshAfterRemote(repoId);
      if (ok) set({ notice: { kind: "pushed", repoId, name, remote } });
    },

    requestDelete: async (repoId, name) => {
      set({ notice: null });
      const info = await commands.tagInfo(repoId, name);
      if (info.status === "error") return fail(repoId, info.error);
      if (!info.data) return;
      set({
        dialog: {
          kind: "delete",
          repoId,
          info: info.data,
          remotes: null,
          remotesError: null,
        },
      });
      const remotes = await commands.remoteTags(repoId, name);
      const now = get().dialog;
      if (now?.kind !== "delete" || now.info.name !== name) return;
      set({
        dialog:
          remotes.status === "ok"
            ? { ...now, remotes: remotes.data }
            : { ...now, remotes: [], remotesError: remotes.error },
      });
    },

    confirmDelete: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "delete") return;
      const { repoId, info } = dialog;
      set({ busy: true, dialog: null });
      const result = await commands.deleteTag(repoId, info.name, info.oid);
      set({ busy: false });
      refreshAfterRemote(repoId);
      if (result.status === "error") return fail(repoId, result.error);
      set({
        notice: {
          kind: "deleted",
          repoId,
          name: info.name,
          stillOn: dialog.remotes ?? [],
        },
      });
    },

    requestDeleteRemote: (repoId, name, tag) =>
      set({ dialog: { kind: "deleteRemote", repoId, name, tag } }),

    confirmDeleteRemote: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "deleteRemote") return;
      const { repoId, name, tag } = dialog;
      set({ busy: true, dialog: null });
      const result = await commands.deleteRemoteTag(
        repoId,
        tag.remote,
        name,
        tag.oid,
      );
      set({ busy: false });
      refreshAfterRemote(repoId);
      if (result.status === "error") return fail(repoId, result.error);
      if (result.data.type === "rejected") {
        set({
          notice: {
            kind: "rejected",
            repoId,
            name,
            remote: tag.remote,
            detail: result.data.detail,
          },
        });
        return;
      }
      set({
        notice: { kind: "deletedRemote", repoId, name, remote: tag.remote },
      });
    },

    closeDialog: () => set({ dialog: null }),
    dismissNotice: () => set({ notice: null }),
  };
});
