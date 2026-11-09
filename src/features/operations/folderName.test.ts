import { setPlatformForTests } from "../../app/platform";
import { folderName } from "./OperationsPanel";

describe("folderName", () => {
  it("takes the last folder of a macOS or Linux path", () => {
    expect(folderName("/Users/ana/src/app")).toBe("app");
    expect(folderName("/Users/ana/src/app/")).toBe("app");
    expect(folderName("/")).toBe("/");
    // "\" is a valid character in a folder name there.
    expect(folderName("/Users/ana/a\\b")).toBe("a\\b");
  });

  // P.8: Windows paths use "\".
  it("takes the last folder of a Windows path", () => {
    setPlatformForTests("windows");
    expect(folderName("C:\\Users\\ana\\src\\app")).toBe("app");
    expect(folderName("C:\\Users\\ana\\src\\app\\")).toBe("app");
    expect(folderName("\\\\server\\share\\repo")).toBe("repo");
  });
});
