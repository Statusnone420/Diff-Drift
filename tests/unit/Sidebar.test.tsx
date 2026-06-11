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
  it("shows analyzed files section label", () => {
    render(
      <Sidebar
        session={makeSession({ changedFiles: 1 })}
        files={[file]}
        otherFiles={[]}
        selectedId={file.id}
        onSelect={vi.fn()}
        watchingSince={null}
        justUpdated={false}
      />,
    );

    expect(screen.getByText("Analyzed files")).toBeInTheDocument();
    expect(screen.queryByTestId("other-file-list")).not.toBeInTheDocument();
  });

  it("lists other changed files by path when present", () => {
    render(
      <Sidebar
        session={makeSession({ changedFiles: 3 })}
        files={[file]}
        otherFiles={["README.md", "docs/CHANGELOG.md"]}
        selectedId={file.id}
        onSelect={vi.fn()}
        watchingSince={null}
        justUpdated={false}
      />,
    );

    expect(screen.getByText("Analyzed files")).toBeInTheDocument();
    expect(screen.getByText("Other changed files")).toBeInTheDocument();
    expect(screen.getByTestId("other-file-list")).toBeInTheDocument();
    expect(screen.getByText("README.md")).toBeInTheDocument();
    expect(screen.getByText("CHANGELOG.md")).toBeInTheDocument();
    // directory portion rendered separately
    expect(screen.getByText("docs/")).toBeInTheDocument();
  });
});
