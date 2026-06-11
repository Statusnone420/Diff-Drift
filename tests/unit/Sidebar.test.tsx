import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { Sidebar } from "../../src/components/Sidebar";
import type { FileEntry } from "../../src/types";
import { makeSession } from "./helpers";

const file: FileEntry = {
  id: "src_auth_ts",
  name: "auth.ts",
  dir: "src/",
  lang: "TypeScript",
  risks: 0,
  summary: "1 modified",
  changedNodes: 1,
  reviewedNodes: 0,
  nodes: [],
};

describe("Sidebar counts", () => {
  it("explains changed files outside the analyzed list", () => {
    render(
      <Sidebar
        session={makeSession({ changedFiles: 3 })}
        files={[file]}
        selectedId={file.id}
        onSelect={vi.fn()}
        watchingSince={null}
        justUpdated={false}
      />,
    );

    expect(screen.getByText("Analyzed files")).toBeInTheDocument();
    expect(screen.getByText(/2 other changed files not shown here/)).toBeInTheDocument();
    expect(screen.getByText(/unsupported type or no analyzable package\/script drift/)).toBeInTheDocument();
  });
});
