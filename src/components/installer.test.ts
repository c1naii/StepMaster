import { describe, expect, it } from "vitest";
import { finalDir } from "./Installer";

describe("finalDir", () => {
  it("always installs into its own StepMaster folder", () => {
    expect(finalDir("C:\\Program Files\\StepMaster")).toBe("C:\\Program Files\\StepMaster");
    expect(finalDir("D:\\Programs")).toBe("D:\\Programs\\StepMaster");
    expect(finalDir("D:\\Programs\\")).toBe("D:\\Programs\\StepMaster");
    expect(finalDir("D:\\")).toBe("D:\\StepMaster");
    expect(finalDir("E:\\stepmaster")).toBe("E:\\stepmaster");
    expect(finalDir("E:\\MyStepMaster")).toBe("E:\\MyStepMaster\\StepMaster");
  });
});
