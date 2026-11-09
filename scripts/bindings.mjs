// `npm run bindings`: regenerates src/ipc/bindings.ts from the Rust commands.
// Sets UPDATE_BINDINGS through Node, not `VAR=1 command`, which cmd.exe on
// Windows does not understand.
import { spawnSync } from "node:child_process";

const run = spawnSync(
  "cargo",
  ["test", "-q", "-p", "gitcraft", "typescript_bindings_are_up_to_date"],
  { stdio: "inherit", env: { ...process.env, UPDATE_BINDINGS: "1" } },
);
if (run.error) throw run.error;
process.exit(run.status ?? 1);
